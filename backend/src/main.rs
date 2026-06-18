mod config;
mod message_bus;
mod models;
mod dynamics;
mod optimization;
mod clickhouse_client;
mod alerts;
mod mqtt_receiver;
mod cam_simulator;
mod force_optimizer;
mod alarm_ws;
mod api;
mod metrics;

use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::{info, error, warn};
use tracing_subscriber::EnvFilter;
use crate::config::{DynamicsConfig, OptimizationConfig};
use crate::message_bus::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info")))
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .json()
        .init();

    info!("Starting 水碓凸轮机构动力学仿真与舂捣力优化系统...");

    metrics::init_metrics();
    info!("Prometheus metrics initialized");

    let dynamics_config = DynamicsConfig::load_default();
    let optimization_config = OptimizationConfig::load_default();
    info!("Configuration loaded");

    let clickhouse_url = std::env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "http://localhost:8123".to_string());
    let clickhouse_db = std::env::var("CLICKHOUSE_DB")
        .unwrap_or_else(|_| "shuidui".to_string());
    let mqtt_broker = std::env::var("MQTT_BROKER")
        .unwrap_or_else(|_| "localhost".to_string());
    let mqtt_port: u16 = std::env::var("MQTT_PORT")
        .unwrap_or_else(|_| "1883".to_string())
        .parse()?;
    let mqtt_topic = std::env::var("MQTT_TOPIC")
        .unwrap_or_else(|_| "shuidui/sensor/#".to_string());
    let api_port: u16 = std::env::var("API_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()?;

    info!(
        clickhouse_url = %clickhouse_url,
        clickhouse_db = %clickhouse_db,
        mqtt_broker = %mqtt_broker,
        mqtt_port = mqtt_port,
        mqtt_topic = %mqtt_topic,
        api_port = api_port,
        "Service configuration"
    );

    let clickhouse = Arc::new(clickhouse_client::ClickHouseClient::new(
        &clickhouse_url,
        &clickhouse_db,
    )?);
    info!("ClickHouse client initialized");

    let devices = clickhouse.get_all_devices().await.unwrap_or_else(|e| {
        error!(error = %e, "Failed to load devices from ClickHouse");
        vec![]
    });
    info!(device_count = devices.len(), "Loaded devices from database");
    metrics::ACTIVE_DEVICES.set(devices.len() as i64);

    let (alert_tx, _alert_rx) = broadcast::channel::<models::Alert>(100);

    let (sensor_tx, _sensor_rx) = mpsc::unbounded_channel::<models::SensorData>();
    let (dynamics_tx, mut dynamics_rx) = mpsc::unbounded_channel::<models::DynamicsResult>();
    let (sim_cmd_tx, sim_cmd_rx) = mpsc::unbounded_channel::<SimulatorCommand>();
    let (opt_cmd_tx, opt_cmd_rx) = mpsc::unbounded_channel::<OptimizerCommand>();
    let (alarm_cmd_tx, alarm_cmd_rx) = mpsc::unbounded_channel::<AlarmCommand>();

    let cam_simulator_service = cam_simulator::CamSimulatorService::new(sim_cmd_rx, dynamics_config.clone())
        .with_devices(&devices);

    let force_optimizer_service = force_optimizer::ForceOptimizerService::new(
        opt_cmd_rx,
        optimization_config.clone(),
        dynamics_config.clone(),
    );

    let alarm_ws_service = alarm_ws::AlarmWsService::new(alarm_cmd_rx, alert_tx.clone());

    let mut mqtt_receiver = mqtt_receiver::MqttReceiver::new(
        &mqtt_broker,
        mqtt_port,
        &mqtt_topic,
        "shuidui-backend",
        clickhouse.clone(),
        sensor_tx,
        dynamics_tx,
        sim_cmd_tx.clone(),
        alarm_cmd_tx.clone(),
    )?;
    mqtt_receiver.set_devices(devices.clone());

    let api_server = api::ApiServer::new(
        clickhouse.clone(),
        alert_tx.clone(),
        sim_cmd_tx,
        opt_cmd_tx,
        alarm_cmd_tx,
    );
    let routes = api_server.routes();

    let sim_handle = tokio::spawn(async move {
        cam_simulator_service.run().await;
    });

    let opt_handle = tokio::spawn(async move {
        force_optimizer_service.run().await;
    });

    let alarm_handle = tokio::spawn(async move {
        alarm_ws_service.run().await;
    });

    let mqtt_handle = tokio::spawn(async move {
        if let Err(e) = mqtt_receiver.run().await {
            error!(error = %e, "MQTT subscriber error");
        }
    });

    let ch_clone = clickhouse.clone();
    let persistence_handle = tokio::spawn(async move {
        while let Some(dynamics) = dynamics_rx.recv().await {
            if let Err(e) = ch_clone.insert_dynamics_result(&[dynamics]).await {
                warn!(error = %e, "ClickHouse persist dynamics failed");
            }
        }
    });

    let api_handle = tokio::spawn(async move {
        info!(port = api_port, "API server starting");
        warp::serve(routes).run(([0, 0, 0, 0], api_port)).await;
    });

    info!("System started successfully!");
    info!(port = api_port, "HTTP API available");
    info!(port = api_port, "WebSocket available at /ws/alerts");

    tokio::select! {
        _ = mqtt_handle => {
            error!("MQTT subscriber exited");
        }
        _ = api_handle => {
            error!("API server exited");
        }
        _ = sim_handle => {
            error!("Cam simulator service exited");
        }
        _ = opt_handle => {
            error!("Force optimizer service exited");
        }
        _ = alarm_handle => {
            error!("Alarm WS service exited");
        }
        _ = persistence_handle => {
            error!("Dynamics persistence exited");
        }
    }

    Ok(())
}
