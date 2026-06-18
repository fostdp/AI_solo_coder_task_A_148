use crate::models::{
    SensorData, DynamicsResult, Alert, DeviceInfo,
    OptimizationRequest, OptimizationResult,
};
use crate::clickhouse_client::ClickHouseClient;
use crate::message_bus::{SimulatorCmdTx, OptimizerCmdTx, AlarmCmdTx, SimulatorCommand, OptimizerCommand};
use crate::metrics;

use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot};
use warp::Filter;
use serde::{Deserialize, Serialize};
use tracing::{error, warn};
use futures_util::StreamExt;

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
    sim_cmd_tx: SimulatorCmdTx,
    opt_cmd_tx: OptimizerCmdTx,
    alarm_cmd_tx: AlarmCmdTx,
}

impl ApiServer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        clickhouse: Arc<ClickHouseClient>,
        alert_tx: broadcast::Sender<Alert>,
        sim_cmd_tx: SimulatorCmdTx,
        opt_cmd_tx: OptimizerCmdTx,
        alarm_cmd_tx: AlarmCmdTx,
    ) -> Self {
        let alert_rx = alert_tx.subscribe();
        ApiServer {
            clickhouse,
            alert_rx,
            alert_tx,
            sim_cmd_tx,
            opt_cmd_tx,
            alarm_cmd_tx,
        }
    }

    pub fn routes(&self) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
        let clickhouse = self.clickhouse.clone();
        let alert_tx = self.alert_tx.clone();
        let sim_cmd_tx = self.sim_cmd_tx.clone();
        let opt_cmd_tx = self.opt_cmd_tx.clone();

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

        let device_by_id_route = warp::path!("api" / "devices" / String)
            .and(warp::get())
            .and(with_clickhouse(clickhouse.clone()))
            .and_then(handle_get_device_info);

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
            .and(with_opt_tx(opt_cmd_tx.clone()))
            .and_then(handle_optimize);

        let cam_profile_route = warp::path!("api" / "cam-profile" / String)
            .and(warp::get())
            .and(warp::query::<CamProfileParams>())
            .and(with_clickhouse(clickhouse.clone()))
            .and(with_sim_tx(sim_cmd_tx.clone()))
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
            .and(with_sim_tx(sim_cmd_tx.clone()))
            .and_then(handle_simulate);

        let metrics_route = warp::path!("metrics")
            .and(warp::get())
            .map(|| {
                warp::reply::with_header(
                    metrics::gather_metrics(),
                    "content-type",
                    "text/plain; version=0.0.4; charset=utf-8",
                )
            });

        health
            .or(devices_route)
            .or(device_by_id_route)
            .or(sensor_data_route)
            .or(dynamics_route)
            .or(alerts_route)
            .or(device_info_route)
            .or(optimize_route)
            .or(cam_profile_route)
            .or(simulate_route)
            .or(ws_route)
            .or(metrics_route)
            .with(cors)
            .with(warp::filters::compression::gzip())
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

fn with_sim_tx(
    tx: SimulatorCmdTx,
) -> impl Filter<Extract = (SimulatorCmdTx,), Error = Infallible> + Clone {
    warp::any().map(move || tx.clone())
}

fn with_opt_tx(
    tx: OptimizerCmdTx,
) -> impl Filter<Extract = (OptimizerCmdTx,), Error = Infallible> + Clone {
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
    opt_cmd_tx: OptimizerCmdTx,
) -> Result<impl warp::Reply, Infallible> {
    let device_id = request.device_id.clone();
    match clickhouse.get_device_info(&device_id).await {
        Ok(Some(device)) => {
            let (tx, rx) = oneshot::channel();
            let cmd = OptimizerCommand::Optimize {
                request,
                device,
                reply: tx,
            };

            if opt_cmd_tx.send(cmd).is_err() {
                return Ok(warp::reply::json(&ApiResponse::<OptimizationResult> {
                    success: false,
                    data: None,
                    message: Some("Optimization service unavailable".to_string()),
                }));
            }

            match rx.await {
                Ok(result) => {
                    if let Err(e) = clickhouse.insert_optimization_result(&result).await {
                        error!("Failed to persist optimization result: {}", e);
                    }
                    Ok(warp::reply::json(&ApiResponse {
                        success: true,
                        data: Some(result),
                        message: None,
                    }))
                }
                Err(e) => Ok(warp::reply::json(&ApiResponse::<OptimizationResult> {
                    success: false,
                    data: None,
                    message: Some(format!("Optimization cancelled: {}", e)),
                })),
            }
        }
        Ok(None) => Ok(warp::reply::json(&ApiResponse::<OptimizationResult> {
            success: false,
            data: None,
            message: Some("Device not found".to_string()),
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<OptimizationResult> {
            success: false,
            data: None,
            message: Some(e.to_string()),
        })),
    }
}

async fn handle_cam_profile(
    device_id: String,
    params: CamProfileParams,
    clickhouse: Arc<ClickHouseClient>,
    sim_cmd_tx: SimulatorCmdTx,
) -> Result<impl warp::Reply, Infallible> {
    use crate::models::CamPoint;
    match clickhouse.get_device_info(&device_id).await {
        Ok(Some(device)) => {
            let base_radius = params.base_radius.unwrap_or(device.cam_base_radius);
            let lift = params.lift.unwrap_or(device.cam_lift);

            let (tx, rx) = oneshot::channel();
            let cmd = SimulatorCommand::GenerateProfile {
                device,
                base_radius,
                lift,
                num_points: 360,
                reply: tx,
            };

            if sim_cmd_tx.send(cmd).is_err() {
                return Ok(warp::reply::json(&ApiResponse::<Vec<CamPoint>> {
                    success: false,
                    data: None,
                    message: Some("Simulator service unavailable".to_string()),
                }));
            }

            match rx.await {
                Ok(profile) => Ok(warp::reply::json(&ApiResponse {
                    success: true,
                    data: Some(profile),
                    message: None,
                })),
                Err(e) => Ok(warp::reply::json(&ApiResponse::<Vec<CamPoint>> {
                    success: false,
                    data: None,
                    message: Some(format!("Profile generation cancelled: {}", e)),
                })),
            }
        }
        Ok(None) => Ok(warp::reply::json(&ApiResponse::<Vec<CamPoint>> {
            success: false,
            data: None,
            message: Some("Device not found".to_string()),
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<Vec<CamPoint>> {
            success: false,
            data: None,
            message: Some(e.to_string()),
        })),
    }
}

async fn handle_simulate(
    request: OptimizationRequest,
    clickhouse: Arc<ClickHouseClient>,
    sim_cmd_tx: SimulatorCmdTx,
) -> Result<impl warp::Reply, Infallible> {
    use crate::models::SensorData;
    let device_id = request.device_id.clone();
    match clickhouse.get_device_info(&device_id).await {
        Ok(Some(device)) => {
            let sensor = SensorData {
                device_id: request.device_id,
                timestamp: chrono::Utc::now(),
                cam_angle: 90.0,
                duitou_acceleration: 0.0,
                grain_reaction_force: 0.0,
                frame_vibration_x: 0.0,
                frame_vibration_y: 0.0,
                frame_vibration_z: 0.0,
                frame_vibration_total: 0.0,
                water_wheel_speed: 3.14,
                duitou_position: 0.06,
            };

            let (tx, rx) = oneshot::channel();
            let cmd = SimulatorCommand::Simulate {
                sensor,
                device,
                reply: tx,
            };

            if sim_cmd_tx.send(cmd).is_err() {
                return Ok(warp::reply::json(&ApiResponse::<DynamicsResult> {
                    success: false,
                    data: None,
                    message: Some("Simulator service unavailable".to_string()),
                }));
            }

            match rx.await {
                Ok(result) => Ok(warp::reply::json(&ApiResponse {
                    success: true,
                    data: Some(result),
                    message: None,
                })),
                Err(e) => Ok(warp::reply::json(&ApiResponse::<DynamicsResult> {
                    success: false,
                    data: None,
                    message: Some(format!("Simulation cancelled: {}", e)),
                })),
            }
        }
        Ok(None) => Ok(warp::reply::json(&ApiResponse::<DynamicsResult> {
            success: false,
            data: None,
            message: Some("Device not found".to_string()),
        })),
        Err(e) => Ok(warp::reply::json(&ApiResponse::<DynamicsResult> {
            success: false,
            data: None,
            message: Some(e.to_string()),
        })),
    }
}

async fn handle_websocket(
    ws: warp::ws::WebSocket,
    mut rx: broadcast::Receiver<Alert>,
) {
    let (mut ws_tx, _ws_rx) = ws.split();
    loop {
        match rx.recv().await {
            Ok(alert) => {
                use futures_util::SinkExt;
                let msg = match serde_json::to_string(&alert) {
                    Ok(json) => warp::ws::Message::text(json),
                    Err(e) => {
                        error!("WebSocket serialize error: {}", e);
                        continue;
                    }
                };
                if let Err(e) = ws_tx.send(msg).await {
                    error!("WebSocket send error: {}", e);
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                warn!("WebSocket lagged, skipped {} alerts", skipped);
                continue;
            }
            Err(broadcast::error::RecvError::Closed) => {
                break;
            }
        }
    }
}
