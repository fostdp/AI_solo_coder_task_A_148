use crate::models::{SensorData, DynamicsResult, Alert, DeviceInfo, OptimizationResult};
use clickhouse::{Client, Row};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use futures_util::TryFutureExt;

#[derive(Debug, Error)]
pub enum ClickHouseError {
    #[error("ClickHouse connection error: {0}")]
    ConnectionError(String),
    #[error("Query error: {0}")]
    QueryError(String),
    #[error("Insert error: {0}")]
    InsertError(String),
}

#[derive(Debug, Clone, Row, Serialize, Deserialize)]
struct SensorRow {
    device_id: String,
    timestamp: i64,
    cam_angle: f64,
    duitou_acceleration: f64,
    grain_reaction_force: f64,
    frame_vibration_x: f64,
    frame_vibration_y: f64,
    frame_vibration_z: f64,
    frame_vibration_total: f64,
    water_wheel_speed: f64,
    duitou_position: f64,
}

#[derive(Debug, Clone, Row, Serialize, Deserialize)]
struct DynamicsRow {
    device_id: String,
    timestamp: i64,
    cam_angle: f64,
    pounding_force: f64,
    impact_energy: f64,
    duitou_velocity: f64,
    duitou_displacement: f64,
    contact_time: f64,
    restitution_coefficient: f64,
    friction_force: f64,
}

#[derive(Debug, Clone, Row, Serialize, Deserialize)]
struct AlertRow {
    id: String,
    device_id: String,
    timestamp: i64,
    alert_type: String,
    alert_level: String,
    alert_message: String,
    alert_value: f64,
    threshold: f64,
    acknowledged: bool,
}

pub struct ClickHouseClient {
    client: Client,
}

impl ClickHouseClient {
    pub fn new(url: &str, database: &str) -> Result<Self, ClickHouseError> {
        let client = Client::default()
            .with_url(url)
            .with_database(database);

        Ok(ClickHouseClient { client })
    }

    pub async fn insert_sensor_data(&self, data: &[SensorData]) -> Result<(), ClickHouseError> {
        let rows: Vec<SensorRow> = data
            .iter()
            .map(|d| SensorRow {
                device_id: d.device_id.clone(),
                timestamp: d.timestamp.timestamp_millis(),
                cam_angle: d.cam_angle,
                duitou_acceleration: d.duitou_acceleration,
                grain_reaction_force: d.grain_reaction_force,
                frame_vibration_x: d.frame_vibration_x,
                frame_vibration_y: d.frame_vibration_y,
                frame_vibration_z: d.frame_vibration_z,
                frame_vibration_total: d.frame_vibration_total,
                water_wheel_speed: d.water_wheel_speed,
                duitou_position: d.duitou_position,
            })
            .collect();

        let mut insert = self.client.insert("sensor_data").map_err(|e| {
            ClickHouseError::InsertError(e.to_string())
        })?;

        for row in rows {
            insert.write(&row).await.map_err(|e| {
                ClickHouseError::InsertError(e.to_string())
            })?;
        }

        insert.end().await.map_err(|e| {
            ClickHouseError::InsertError(e.to_string())
        })?;

        Ok(())
    }

    pub async fn insert_dynamics_result(
        &self,
        results: &[DynamicsResult],
    ) -> Result<(), ClickHouseError> {
        let rows: Vec<DynamicsRow> = results
            .iter()
            .map(|d| DynamicsRow {
                device_id: d.device_id.clone(),
                timestamp: d.timestamp.timestamp_millis(),
                cam_angle: d.cam_angle,
                pounding_force: d.pounding_force,
                impact_energy: d.impact_energy,
                duitou_velocity: d.duitou_velocity,
                duitou_displacement: d.duitou_displacement,
                contact_time: d.contact_time,
                restitution_coefficient: d.restitution_coefficient,
                friction_force: d.friction_force,
            })
            .collect();

        let mut insert = self.client.insert("dynamics_simulation").map_err(|e| {
            ClickHouseError::InsertError(e.to_string())
        })?;

        for row in rows {
            insert.write(&row).await.map_err(|e| {
                ClickHouseError::InsertError(e.to_string())
            })?;
        }

        insert.end().await.map_err(|e| {
            ClickHouseError::InsertError(e.to_string())
        })?;

        Ok(())
    }

    pub async fn insert_alert(&self, alert: &Alert) -> Result<(), ClickHouseError> {
        let row = AlertRow {
            id: alert.id.clone(),
            device_id: alert.device_id.clone(),
            timestamp: alert.timestamp.timestamp_millis(),
            alert_type: alert.alert_type.clone(),
            alert_level: alert.alert_level.clone(),
            alert_message: alert.alert_message.clone(),
            alert_value: alert.alert_value,
            threshold: alert.threshold,
            acknowledged: false,
        };

        let mut insert = self.client.insert("alerts").map_err(|e| {
            ClickHouseError::InsertError(e.to_string())
        })?;

        insert.write(&row).await.map_err(|e| {
            ClickHouseError::InsertError(e.to_string())
        })?;

        insert.end().await.map_err(|e| {
            ClickHouseError::InsertError(e.to_string())
        })?;

        Ok(())
    }

    pub async fn query_recent_sensor_data(
        &self,
        device_id: &str,
        limit: u64,
    ) -> Result<Vec<SensorData>, ClickHouseError> {
        let query = format!(
            "SELECT device_id, timestamp, cam_angle, duitou_acceleration, grain_reaction_force,
                    frame_vibration_x, frame_vibration_y, frame_vibration_z, frame_vibration_total,
                    water_wheel_speed, duitou_position
             FROM sensor_data
             WHERE device_id = '{}'
             ORDER BY timestamp DESC
             LIMIT {}",
            device_id, limit
        );

        let rows: Vec<SensorRow> = self
            .client
            .query(&query)
            .fetch_all()
            .await
            .map_err(|e| ClickHouseError::QueryError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| SensorData {
                device_id: r.device_id,
                timestamp: chrono::DateTime::from_timestamp_millis(r.timestamp)
                    .unwrap_or_else(|| chrono::Utc::now()),
                cam_angle: r.cam_angle,
                duitou_acceleration: r.duitou_acceleration,
                grain_reaction_force: r.grain_reaction_force,
                frame_vibration_x: r.frame_vibration_x,
                frame_vibration_y: r.frame_vibration_y,
                frame_vibration_z: r.frame_vibration_z,
                frame_vibration_total: r.frame_vibration_total,
                water_wheel_speed: r.water_wheel_speed,
                duitou_position: r.duitou_position,
            })
            .collect())
    }

    pub async fn query_recent_dynamics(
        &self,
        device_id: &str,
        limit: u64,
    ) -> Result<Vec<DynamicsResult>, ClickHouseError> {
        let query = format!(
            "SELECT device_id, timestamp, cam_angle, pounding_force, impact_energy,
                    duitou_velocity, duitou_displacement, contact_time,
                    restitution_coefficient, friction_force
             FROM dynamics_simulation
             WHERE device_id = '{}'
             ORDER BY timestamp DESC
             LIMIT {}",
            device_id, limit
        );

        let rows: Vec<DynamicsRow> = self
            .client
            .query(&query)
            .fetch_all()
            .await
            .map_err(|e| ClickHouseError::QueryError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| DynamicsResult {
                device_id: r.device_id,
                timestamp: chrono::DateTime::from_timestamp_millis(r.timestamp)
                    .unwrap_or_else(|| chrono::Utc::now()),
                cam_angle: r.cam_angle,
                pounding_force: r.pounding_force,
                impact_energy: r.impact_energy,
                duitou_velocity: r.duitou_velocity,
                duitou_displacement: r.duitou_displacement,
                contact_time: r.contact_time,
                restitution_coefficient: r.restitution_coefficient,
                friction_force: r.friction_force,
            })
            .collect())
    }

    pub async fn query_recent_alerts(
        &self,
        device_id: Option<&str>,
        limit: u64,
    ) -> Result<Vec<Alert>, ClickHouseError> {
        let query = match device_id {
            Some(id) => format!(
                "SELECT id, device_id, timestamp, alert_type, alert_level, alert_message,
                        alert_value, threshold, acknowledged
                 FROM alerts
                 WHERE device_id = '{}'
                 ORDER BY timestamp DESC
                 LIMIT {}",
                id, limit
            ),
            None => format!(
                "SELECT id, device_id, timestamp, alert_type, alert_level, alert_message,
                        alert_value, threshold, acknowledged
                 FROM alerts
                 ORDER BY timestamp DESC
                 LIMIT {}",
                limit
            ),
        };

        let rows: Vec<AlertRow> = self
            .client
            .query(&query)
            .fetch_all()
            .await
            .map_err(|e| ClickHouseError::QueryError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| Alert {
                id: r.id,
                device_id: r.device_id,
                timestamp: chrono::DateTime::from_timestamp_millis(r.timestamp)
                    .unwrap_or_else(|| chrono::Utc::now()),
                alert_type: r.alert_type,
                alert_level: r.alert_level,
                alert_message: r.alert_message,
                alert_value: r.alert_value,
                threshold: r.threshold,
            })
            .collect())
    }

    pub async fn get_device_info(
        &self,
        device_id: &str,
    ) -> Result<Option<DeviceInfo>, ClickHouseError> {
        let query = format!(
            "SELECT device_id, device_name, location, cam_base_radius, cam_lift,
                    duitou_mass, water_flow_rate, frame_vibration_threshold
             FROM devices
             WHERE device_id = '{}'
             LIMIT 1",
            device_id
        );

        #[derive(Debug, Row, Deserialize)]
        struct DeviceRow {
            device_id: String,
            device_name: String,
            location: String,
            cam_base_radius: f64,
            cam_lift: f64,
            duitou_mass: f64,
            water_flow_rate: f64,
            frame_vibration_threshold: f64,
        }

        let result: Option<DeviceRow> = self
            .client
            .query(&query)
            .fetch_optional()
            .await
            .map_err(|e| ClickHouseError::QueryError(e.to_string()))?;

        Ok(result.map(|r| DeviceInfo {
            device_id: r.device_id,
            device_name: r.device_name,
            location: r.location,
            cam_base_radius: r.cam_base_radius,
            cam_lift: r.cam_lift,
            duitou_mass: r.duitou_mass,
            water_flow_rate: r.water_flow_rate,
            frame_vibration_threshold: r.frame_vibration_threshold,
        }))
    }

    pub async fn get_all_devices(&self) -> Result<Vec<DeviceInfo>, ClickHouseError> {
        let query = r#"
            SELECT device_id, device_name, location, cam_base_radius, cam_lift,
                   duitou_mass, water_flow_rate, frame_vibration_threshold
            FROM devices
            WHERE is_active = true
            ORDER BY device_id
        "#;

        #[derive(Debug, Row, Deserialize)]
        struct DeviceRow {
            device_id: String,
            device_name: String,
            location: String,
            cam_base_radius: f64,
            cam_lift: f64,
            duitou_mass: f64,
            water_flow_rate: f64,
            frame_vibration_threshold: f64,
        }

        let rows: Vec<DeviceRow> = self
            .client
            .query(query)
            .fetch_all()
            .await
            .map_err(|e| ClickHouseError::QueryError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| DeviceInfo {
                device_id: r.device_id,
                device_name: r.device_name,
                location: r.location,
                cam_base_radius: r.cam_base_radius,
                cam_lift: r.cam_lift,
                duitou_mass: r.duitou_mass,
                water_flow_rate: r.water_flow_rate,
                frame_vibration_threshold: r.frame_vibration_threshold,
            })
            .collect())
    }

    pub async fn insert_optimization_result(
        &self,
        result: &OptimizationResult,
    ) -> Result<(), ClickHouseError> {
        #[derive(Debug, Row, Serialize)]
        struct OptRow {
            id: String,
            device_id: String,
            timestamp: i64,
            cam_base_radius: f64,
            cam_lift: f64,
            cam_pressure_angle: f64,
            cam_profile_type: String,
            target_efficiency: f64,
            actual_efficiency: f64,
            average_pounding_force: f64,
            impact_energy_per_cycle: f64,
            husking_rate: f64,
            grain_breakage_rate: f64,
            optimization_parameters: String,
        }

        let params = serde_json::to_string(&result.cam_profile)
            .unwrap_or_else(|_| "[]".to_string());

        let row = OptRow {
            id: result.optimization_id.clone(),
            device_id: result.device_id.clone(),
            timestamp: result.timestamp.timestamp_millis(),
            cam_base_radius: result.base_radius,
            cam_lift: result.lift,
            cam_pressure_angle: 0.0,
            cam_profile_type: result.cam_profile_type.clone(),
            target_efficiency: 0.0,
            actual_efficiency: result.overall_efficiency,
            average_pounding_force: result.pounding_force,
            impact_energy_per_cycle: result.impact_energy,
            husking_rate: result.husking_rate,
            grain_breakage_rate: result.breakage_rate,
            optimization_parameters: params,
        };

        let mut insert = self
            .client
            .insert("optimization_results")
            .map_err(|e| ClickHouseError::InsertError(e.to_string()))?;

        insert
            .write(&row)
            .await
            .map_err(|e| ClickHouseError::InsertError(e.to_string()))?;

        insert
            .end()
            .await
            .map_err(|e| ClickHouseError::InsertError(e.to_string()))?;

        Ok(())
    }
}
