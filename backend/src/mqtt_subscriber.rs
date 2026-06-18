use crate::models::{MqttSensorMessage, SensorData, DeviceInfo};
use crate::dynamics::CamDynamicsSimulator;
use crate::alerts::AlertDetector;
use crate::clickhouse_client::ClickHouseClient;
use crate::models::Alert;

use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use serde_json;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::broadcast;
use parking_lot::Mutex;
use log::{info, warn, error};

pub struct MqttSubscriber {
    client: AsyncClient,
    event_loop: EventLoop,
    clickhouse: Arc<ClickHouseClient>,
    alert_tx: broadcast::Sender<Alert>,
    device_info: HashMap<String, DeviceInfo>,
    simulators: Arc<Mutex<HashMap<String, CamDynamicsSimulator>>>,
    topic: String,
}

impl MqttSubscriber {
    pub fn new(
        broker_url: &str,
        port: u16,
        topic: &str,
        client_id: &str,
        clickhouse: Arc<ClickHouseClient>,
        alert_tx: broadcast::Sender<Alert>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut options = MqttOptions::new(client_id, broker_url, port);
        options.set_keep_alive(std::time::Duration::from_secs(30));

        let (client, event_loop) = AsyncClient::new(options, 10);

        Ok(MqttSubscriber {
            client,
            event_loop,
            clickhouse,
            alert_tx,
            device_info: HashMap::new(),
            simulators: Arc::new(Mutex::new(HashMap::new())),
            topic: topic.to_string(),
        })
    }

    pub fn set_devices(&mut self, devices: Vec<DeviceInfo>) {
        let mut sims = self.simulators.lock();
        for device in devices {
            let simulator = CamDynamicsSimulator::new(device.clone());
            sims.insert(device.device_id.clone(), simulator);
            self.device_info.insert(device.device_id.clone(), device);
        }
    }

    pub async fn subscribe(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.client
            .subscribe(&self.topic, QoS::AtLeastOnce)
            .await?;
        info!("Subscribed to MQTT topic: {}", self.topic);
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.subscribe().await?;

        let mut sensor_buffer: Vec<SensorData> = Vec::new();
        let mut dynamics_buffer: Vec<crate::models::DynamicsResult> = Vec::new();
        let buffer_size = 10;

        loop {
            match self.event_loop.poll().await {
                Ok(notification) => {
                    if let Event::Incoming(Packet::Publish(publish)) = notification {
                        match serde_json::from_slice::<MqttSensorMessage>(&publish.payload) {
                            Ok(msg) => {
                                let sensor_data: SensorData = msg.into();
                                info!(
                                    "Received sensor data from {}: cam_angle={:.2}",
                                    sensor_data.device_id, sensor_data.cam_angle
                                );

                                let device_id = sensor_data.device_id.clone();

                                if let Some(device) = self.device_info.get(&device_id) {
                                    let dynamics = {
                                        let mut sims = self.simulators.lock();
                                        if !sims.contains_key(&device_id) {
                                            sims.insert(
                                                device_id.clone(),
                                                CamDynamicsSimulator::new(device.clone()),
                                            );
                                        }
                                        let sim = sims.get_mut(&device_id).unwrap();
                                        sim.simulate(&sensor_data)
                                    };

                                    let detector = AlertDetector::new(device);
                                    let alerts = detector.detect(&sensor_data);

                                    for alert in alerts {
                                        let alert_msg = format!(
                                            "ALERT [{}] {} - {}",
                                            alert.alert_level,
                                            alert.alert_type,
                                            alert.alert_message
                                        );
                                        warn!("{}", alert_msg);

                                        let _ = self.alert_tx.send(alert.clone());

                                        let ch = self.clickhouse.clone();
                                        tokio::spawn(async move {
                                            if let Err(e) = ch.insert_alert(&alert).await {
                                                error!("Failed to insert alert: {}", e);
                                            }
                                        });
                                    }

                                    sensor_buffer.push(sensor_data);
                                    dynamics_buffer.push(dynamics);

                                    if sensor_buffer.len() >= buffer_size {
                                        let sensor_batch =
                                            std::mem::take(&mut sensor_buffer);
                                        let dynamics_batch =
                                            std::mem::take(&mut dynamics_buffer);

                                        let ch = self.clickhouse.clone();
                                        tokio::spawn(async move {
                                            if let Err(e) = ch.insert_sensor_data(&sensor_batch).await {
                                                error!("Failed to insert sensor data: {}", e);
                                            }
                                            if let Err(e) = ch.insert_dynamics_result(&dynamics_batch).await {
                                                error!("Failed to insert dynamics result: {}", e);
                                            }
                                        });
                                    }
                                } else {
                                    warn!("Unknown device: {}", sensor_data.device_id);
                                }
                            }
                            Err(e) => {
                                error!("Failed to parse MQTT message: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("MQTT connection error: {}", e);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    }
}
