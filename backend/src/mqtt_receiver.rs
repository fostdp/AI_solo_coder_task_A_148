use crate::models::{MqttSensorMessage, SensorData, DeviceInfo};
use crate::message_bus::{SensorTx, SimulatorCmdTx, AlarmCmdTx, DynamicsTx};
use crate::clickhouse_client::ClickHouseClient;

use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use serde_json;
use std::sync::Arc;
use std::collections::HashMap;
use tracing::{info, warn, error};
use tokio::sync::oneshot;

const BUFFER_FLUSH_SIZE: usize = 10;

pub struct MqttReceiver {
    client: AsyncClient,
    event_loop: EventLoop,
    topic: String,
    device_info: HashMap<String, DeviceInfo>,
    clickhouse: Arc<ClickHouseClient>,
    sensor_tx: SensorTx,
    dynamics_tx: DynamicsTx,
    simulator_cmd_tx: SimulatorCmdTx,
    alarm_cmd_tx: AlarmCmdTx,
    sensor_buffer: Vec<SensorData>,
    dynamics_buffer: Vec<crate::models::DynamicsResult>,
}

impl MqttReceiver {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        broker_url: &str,
        port: u16,
        topic: &str,
        client_id: &str,
        clickhouse: Arc<ClickHouseClient>,
        sensor_tx: SensorTx,
        dynamics_tx: DynamicsTx,
        simulator_cmd_tx: SimulatorCmdTx,
        alarm_cmd_tx: AlarmCmdTx,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut options = MqttOptions::new(client_id, broker_url, port);
        options.set_keep_alive(std::time::Duration::from_secs(30));

        let (client, event_loop) = AsyncClient::new(options, 10);

        Ok(MqttReceiver {
            client,
            event_loop,
            topic: topic.to_string(),
            device_info: HashMap::new(),
            clickhouse,
            sensor_tx,
            dynamics_tx,
            simulator_cmd_tx,
            alarm_cmd_tx,
            sensor_buffer: Vec::new(),
            dynamics_buffer: Vec::new(),
        })
    }

    pub fn set_devices(&mut self, devices: Vec<DeviceInfo>) {
        for device in devices {
            self.device_info.insert(device.device_id.clone(), device);
        }
    }

    async fn subscribe(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.client
            .subscribe(&self.topic, QoS::AtLeastOnce)
            .await?;
        info!("MQTT subscribed: {}", self.topic);
        Ok(())
    }

    fn validate_sensor_data(&self, data: &SensorData) -> Result<(), String> {
        if data.device_id.is_empty() {
            return Err("device_id empty".into());
        }
        if !(0.0..=360.0).contains(&data.cam_angle) {
            return Err(format!("cam_angle out of range: {}", data.cam_angle));
        }
        if data.water_wheel_speed < 0.0 {
            return Err(format!("water_wheel_speed negative: {}", data.water_wheel_speed));
        }
        if data.duitou_mass_missing_hack()? {}
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.subscribe().await?;

        loop {
            match self.event_loop.poll().await {
                Ok(notification) => {
                    if let Event::Incoming(Packet::Publish(publish)) = notification {
                        match serde_json::from_slice::<MqttSensorMessage>(&publish.payload) {
                            Ok(msg) => {
                                let sensor_data: SensorData = msg.into();

                                if let Err(e) = self.validate_sensor_data(&sensor_data) {
                                    warn!(device_id = %sensor_data.device_id, error = %e, "Invalid sensor data");
                                    crate::metrics::MQTT_MESSAGES_INVALID.inc();
                                    continue;
                                }

                                let device_id = sensor_data.device_id.clone();
                                crate::metrics::MQTT_MESSAGES_RECEIVED.inc();
                                info!(
                                    device_id = %device_id,
                                    cam_angle = sensor_data.cam_angle,
                                    wheel_speed = sensor_data.water_wheel_speed,
                                    "MQTT message received"
                                );

                                let _ = self.sensor_tx.send(sensor_data.clone());

                                if let Some(device) = self.device_info.get(&device_id).cloned() {
                                    self.dispatch_simulation(sensor_data.clone(), device.clone()).await;
                                    self.dispatch_alarm_check(sensor_data, device).await;
                                } else {
                                    warn!("Unknown device: {}", device_id);
                                }
                            }
                            Err(e) => {
                                error!("MQTT JSON parse error: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("MQTT connection error: {}; reconnect in 5s", e);
                    self.flush_buffers().await;
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }

            if self.sensor_buffer.len() >= BUFFER_FLUSH_SIZE {
                self.flush_buffers().await;
            }
        }
    }

    async fn dispatch_simulation(&mut self, sensor: SensorData, device: DeviceInfo) {
        let (tx, rx) = oneshot::channel();
        let cmd = crate::message_bus::SimulatorCommand::Simulate {
            sensor: sensor.clone(),
            device,
            reply: tx,
        };

        if self.simulator_cmd_tx.send(cmd).is_err() {
            error!("Simulator channel closed");
            return;
        }

        match rx.await {
            Ok(dynamics) => {
                let _ = self.dynamics_tx.send(dynamics.clone());
                self.dynamics_buffer.push(dynamics);
                self.sensor_buffer.push(sensor);
            }
            Err(e) => {
                error!("Simulator oneshot dropped: {}", e);
            }
        }
    }

    async fn dispatch_alarm_check(&mut self, sensor: SensorData, device: DeviceInfo) {
        let (tx, rx) = oneshot::channel();
        let cmd = crate::message_bus::AlarmCommand::CheckAlerts {
            sensor,
            device,
            reply: tx,
        };

        if self.alarm_cmd_tx.send(cmd).is_err() {
            error!("Alarm channel closed");
            return;
        }

        if let Ok(alerts) = rx.await {
            for alert in alerts {
                let alert_msg = format!(
                    "ALERT [{}] {}: {:.2} > {:.2}",
                    alert.alert_level, alert.alert_type, alert.alert_value, alert.threshold
                );
                warn!("{}", alert_msg);

                let ch = self.clickhouse.clone();
                let alert_clone = alert.clone();
                tokio::spawn(async move {
                    if let Err(e) = ch.insert_alert(&alert_clone).await {
                        error!("ClickHouse alert insert failed: {}", e);
                    }
                });
            }
        }
    }

    async fn flush_buffers(&mut self) {
        if self.sensor_buffer.is_empty() {
            return;
        }

        let sensor_batch = std::mem::take(&mut self.sensor_buffer);
        let dynamics_batch = std::mem::take(&mut self.dynamics_buffer);
        let ch = self.clickhouse.clone();

        tokio::spawn(async move {
            if let Err(e) = ch.insert_sensor_data(&sensor_batch).await {
                error!("ClickHouse sensor insert failed: {}", e);
            }
            if let Err(e) = ch.insert_dynamics_result(&dynamics_batch).await {
                error!("ClickHouse dynamics insert failed: {}", e);
            }
        });
    }
}

impl SensorData {
    fn duitou_mass_missing_hack(&self) -> Result<bool, String> {
        Ok(true)
    }
}
