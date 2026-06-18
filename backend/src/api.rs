use crate::models::{
    SensorData, DynamicsResult, Alert, DeviceInfo,
    OptimizationRequest, OptimizationResult,
};
use crate::clickhouse_client::ClickHouseClient;
use crate::optimization::PoundingOptimizer;
use crate::dynamics::CamDynamicsSimulator;

use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::broadcast;
use warp::Filter;
use serde::{Deserialize, Serialize};
use log::{info, error};

#[derive(Debug, Serialize, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct QueryParams {
    device_id: Option<String>,
    limit: Option<u64>,
}

pub struct ApiServer {
    clickhouse: Arc<ClickHouseClient>,
    alert_rx: broadcast::Receiver<Alert>,
    alert_tx: broadcast::Sender<Alert>,
}

impl ApiServer {
    pub fn new(
        clickhouse: Arc<ClickHouseClient>,
        alert_tx: broadcast::Sender<Alert>,
    ) -> Self {
        let alert_rx = alert_tx.subscribe();
        ApiServer {
            clickhouse,
            alert_rx,
            alert_tx,
        }
    }

    pub fn routes(&self) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        let clickhouse = self.clickhouse.clone();
        let alert_tx = self.alert_tx.clone();

        let cors = warp::cors()
            .allow_any_origin()
            .allow_headers(vec!["Content-Type", "Authorization"])
            .allow_methods(vec!["GET", "POST", "OPTIONS"]);

        let health = warp::path!("health")
            .and(warp::get())
            .map(|| {
                warp::reply::json(&ApiResponse::<()> {
                    success: true,
                    data: None,
                    message: Some("OK".to_string()),
                })
            });

        let devices_route = warp::path!("api" / "devices")
            .and(warp::get())
            .and(with_clickhouse(clickhouse.clone()))
            .and_then(handle_get_devices);

        let sensor_data_route = warp::path!("api" / "sensor-data")
            .and(warp::get())
            .and(warp::query::<QueryParams>())
            .and(with_clickhouse(clickhouse.clone()))
            .and_then(handle_get_sensor_data);

        let dynamics_route = warp::path!("api" / "dynamics")
            .and(warp::get())
            .and(warp::query::<QueryParams>())
            .and(with_clickhouse(clickhouse.clone()))
            .and_then(handle_get_dynamics);

        let alerts_route = warp::path!("api" / "alerts")
            .and(warp::get())
            .and(warp::query::<QueryParams>())
            .and(with_clickhouse(clickhouse.clone()))
            .and_then(handle_get_alerts);

        let device_info_route = warp::path!("api" / "device" / String)
            .and(warp::get())
            .and(with_clickhouse(clickhouse.clone()))
            .and_then(handle_get_device_info);

        let optimize_route = warp::path!("api" / "optimize")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_clickhouse(clickhouse.clone()))
            .and_then(handle_optimize);

        let cam_profile_route = warp::path!("api" / "cam-profile" / String)
            .and(warp::get())
            .and(warp::query::<CamProfileParams>())
            .and(with_clickhouse(clickhouse.clone()))
            .and_then(handle_cam_profile);

        let ws_route = warp::path!("ws" / "alerts")
            .and(warp::ws())
            .and(with_alert_tx(alert_tx.clone()))
            .map(|ws: warp::ws::Ws, alert_tx: broadcast::Sender<Alert>| {
                ws.on_upgrade(move |socket| handle_websocket(socket, alert_tx.subscribe()))
            });

        let simulate_route = warp::path!("api" / "simulate")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_clickhouse(clickhouse.clone()))
            .and_then(handle_simulate);

        health
            .or(devices_route)
            .or(sensor_data_route)
            .or(dynamics_route)
            .or(alerts_route)
            .or(device_info_route)
            .or(optimize_route)
            .or(cam_profile_route)
            .or(simulate_route)
            .or(ws_route)
            .with(cors)
    }

    pub async fn start(&self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        let routes = self.routes();
        info!("Starting API server on port {}", port);
        warp::serve(routes).run(([0, 0, 0, 0], port)).await;
        Ok(())
    }
}

fn with_clickhouse(
    ch: Arc<ClickHouseClient>,
) -> impl Filter<Extract = (Arc<ClickHouseClient>,), Error = Infallible> + Clone {
    warp::any().map(move || ch.clone())
}

fn with_alert_tx(
    tx: broadcast::Sender<Alert>,
) -> impl Filter<Extract = (broadcast::Sender<Alert>,), Error = Infallible> + Clone {
    warp::any().map(move || tx.clone())
}

#[derive(Debug, Deserialize)]
struct CamProfileParams {
    base_radius: Option<f64>,
    lift: Option<f64>,
}

async fn handle_get_devices(
    clickhouse: Arc<ClickHouseClient>,
) -> Result<impl warp::Reply, Infallible> {
    match clickhouse.get_all_devices().await {
        Ok(devices) => Ok(warp::reply::json(&ApiResponse {
            success: true,
            data: Some(devices),
            message: None,
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<Vec<DeviceInfo>> {
            success: false,
            data: None,
            message: Some(e.to_string()),
        })),
    }
}

async fn handle_get_sensor_data(
    params: QueryParams,
    clickhouse: Arc<ClickHouseClient>,
) -> Result<impl warp::Reply, Infallible> {
    let device_id = params.device_id.unwrap_or_else(|| "shuidui-001".to_string());
    let limit = params.limit.unwrap_or(100);

    match clickhouse.query_recent_sensor_data(&device_id, limit).await {
        Ok(data) => Ok(warp::reply::json(&ApiResponse {
            success: true,
            data: Some(data),
            message: None,
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<Vec<SensorData>> {
            success: false,
            data: None,
            message: Some(e.to_string()),
        })),
    }
}

async fn handle_get_dynamics(
    params: QueryParams,
    clickhouse: Arc<ClickHouseClient>,
) -> Result<impl warp::Reply, Infallible> {
    let device_id = params.device_id.unwrap_or_else(|| "shuidui-001".to_string());
    let limit = params.limit.unwrap_or(100);

    match clickhouse.query_recent_dynamics(&device_id, limit).await {
        Ok(data) => Ok(warp::reply::json(&ApiResponse {
            success: true,
            data: Some(data),
            message: None,
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<Vec<DynamicsResult>> {
            success: false,
            data: None,
            message: Some(e.to_string()),
        })),
    }
}

async fn handle_get_alerts(
    params: QueryParams,
    clickhouse: Arc<ClickHouseClient>,
) -> Result<impl warp::Reply, Infallible> {
    let limit = params.limit.unwrap_or(50);

    match clickhouse.query_recent_alerts(params.device_id.as_deref(), limit).await {
        Ok(data) => Ok(warp::reply::json(&ApiResponse {
            success: true,
            data: Some(data),
            message: None,
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<Vec<Alert>> {
            success: false,
            data: None,
            message: Some(e.to_string()),
        })),
    }
}

async fn handle_get_device_info(
    device_id: String,
    clickhouse: Arc<ClickHouseClient>,
) -> Result<impl warp::Reply, Infallible> {
    match clickhouse.get_device_info(&device_id).await {
        Ok(Some(device)) => Ok(warp::reply::json(&ApiResponse {
            success: true,
            data: Some(device),
            message: None,
        })),
        Ok(None) => Ok(warp::reply::json(&ApiResponse::<DeviceInfo> {
            success: false,
            data: None,
            message: Some("Device not found".to_string()),
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<DeviceInfo> {
            success: false,
            data: None,
            message: Some(e.to_string()),
        })),
    }
}

async fn handle_optimize(
    request: OptimizationRequest,
    clickhouse: Arc<ClickHouseClient>,
) -> Result<impl warp::Reply, Infallible> {
    let device = match clickhouse.get_device_info(&request.device_id).await {
        Ok(Some(d)) => d,
        Ok(None) => {
            return Ok(warp::reply::json(&ApiResponse::<OptimizationResult> {
                success: false,
                data: None,
                message: Some("Device not found".to_string()),
            }));
        }
        Err(e) => {
            return Ok(warp::reply::json(&ApiResponse::<OptimizationResult> {
                success: false,
                data: None,
                message: Some(e.to_string()),
            }));
        }
    };

    let optimizer = PoundingOptimizer::new(device);
    let result = optimizer.optimize(&request);

    let ch_clone = clickhouse.clone();
    let result_clone = result.clone();
    tokio::spawn(async move {
        if let Err(e) = ch_clone.insert_optimization_result(&result_clone).await {
            error!("Failed to insert optimization result: {}", e);
        }
    });

    Ok(warp::reply::json(&ApiResponse {
        success: true,
        data: Some(result),
        message: None,
    }))
}

async fn handle_cam_profile(
    device_id: String,
    params: CamProfileParams,
    clickhouse: Arc<ClickHouseClient>,
) -> Result<impl warp::Reply, Infallible> {
    use crate::models::CamPoint;

    let device = match clickhouse.get_device_info(&device_id).await {
        Ok(Some(d)) => d,
        Ok(None) => {
            return Ok(warp::reply::json(&ApiResponse::<Vec<CamPoint>> {
                success: false,
                data: None,
                message: Some("Device not found".to_string()),
            }));
        }
        Err(e) => {
            return Ok(warp::reply::json(&ApiResponse::<Vec<CamPoint>> {
                success: false,
                data: None,
                message: Some(e.to_string()),
            }));
        }
    };

    let base_radius = params.base_radius.unwrap_or(device.cam_base_radius);
    let lift = params.lift.unwrap_or(device.cam_lift);

    let simulator = CamDynamicsSimulator::new(device);
    let profile = simulator.generate_cam_profile(base_radius, lift, 360);

    Ok(warp::reply::json(&ApiResponse {
        success: true,
        data: Some(profile),
        message: None,
    }))
}

#[derive(Debug, Deserialize)]
struct SimulateRequest {
    pub device_id: String,
    pub cam_angle: f64,
    pub water_wheel_speed: f64,
}

async fn handle_simulate(
    request: SimulateRequest,
    clickhouse: Arc<ClickHouseClient>,
) -> Result<impl warp::Reply, Infallible> {
    let device = match clickhouse.get_device_info(&request.device_id).await {
        Ok(Some(d)) => d,
        Ok(None) => {
            return Ok(warp::reply::json(&ApiResponse::<DynamicsResult> {
                success: false,
                data: None,
                message: Some("Device not found".to_string()),
            }));
        }
        Err(e) => {
            return Ok(warp::reply::json(&ApiResponse::<DynamicsResult> {
                success: false,
                data: None,
                message: Some(e.to_string()),
            }));
        }
    };

    let simulator = CamDynamicsSimulator::new(device);

    let sensor = SensorData {
        device_id: request.device_id,
        timestamp: chrono::Utc::now(),
        cam_angle: request.cam_angle,
        duitou_acceleration: 0.0,
        grain_reaction_force: 0.0,
        frame_vibration_x: 0.0,
        frame_vibration_y: 0.0,
        frame_vibration_z: 0.0,
        frame_vibration_total: 0.0,
        water_wheel_speed: request.water_wheel_speed,
        duitou_position: 0.0,
    };

    let result = simulator.simulate(&sensor);

    Ok(warp::reply::json(&ApiResponse {
        success: true,
        data: Some(result),
        message: None,
    }))
}

async fn handle_websocket(
    ws: warp::ws::WebSocket,
    mut rx: broadcast::Receiver<Alert>,
) {
    let (mut ws_tx, _ws_rx) = ws.split();

    use futures_util::StreamExt;
    use futures_util::sink::SinkExt;

    loop {
        match rx.recv().await {
            Ok(alert) => {
                let msg = serde_json::to_string(&alert).unwrap_or_else(|e| {
                    format!("{{\"error\":\"{}\"}}", e)
                });

                if let Err(e) = ws_tx.send(warp::ws::Message::text(msg)).await {
                    error!("WebSocket send error: {}", e);
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                error!("WebSocket lagged by {} messages", n);
            }
            Err(broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }
}
