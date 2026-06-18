mod models;
mod dynamics;
mod optimization;
mod clickhouse_client;
mod alerts;
mod mqtt_subscriber;
mod api;

use std::sync::Arc;
use tokio::sync::broadcast;
use log::{info, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    info!("Starting 水碓凸轮机构动力学仿真与舂捣力优化系统...");

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

    info!("ClickHouse: {}/{}", clickhouse_url, clickhouse_db);
    info!("MQTT: {}:{} topic={}", mqtt_broker, mqtt_port, mqtt_topic);
    info!("API Port: {}", api_port);

    let clickhouse = Arc::new(clickhouse_client::ClickHouseClient::new(
        &clickhouse_url,
        &clickhouse_db,
    )?);

    info!("ClickHouse client initialized");

    let (alert_tx, _alert_rx) = broadcast::channel::<models::Alert>(100);

    let devices = clickhouse.get_all_devices().await.unwrap_or_else(|e| {
        error!("Failed to load devices from ClickHouse: {}", e);
        vec![]
    });

    info!("Loaded {} devices from database", devices.len());

    let mut subscriber = mqtt_subscriber::MqttSubscriber::new(
        &mqtt_broker,
        mqtt_port,
        &mqtt_topic,
        "shuidui-backend",
        clickhouse.clone(),
        alert_tx.clone(),
    )?;

    subscriber.set_devices(devices);

    let api_server = api::ApiServer::new(clickhouse.clone(), alert_tx.clone());
    let routes = api_server.routes();

    let mqtt_handle = tokio::spawn(async move {
        if let Err(e) = subscriber.run().await {
            error!("MQTT subscriber error: {}", e);
        }
    });

    let api_handle = tokio::spawn(async move {
        info!("API server starting on port {}", api_port);
        warp::serve(routes).run(([0, 0, 0, 0], api_port)).await;
    });

    info!("System started successfully!");
    info!("HTTP API: http://localhost:{}", api_port);
    info!("WebSocket: ws://localhost:{}/ws/alerts", api_port);

    tokio::select! {
        _ = mqtt_handle => {
            error!("MQTT subscriber exited");
        }
        _ = api_handle => {
            error!("API server exited");
        }
    }

    Ok(())
}
