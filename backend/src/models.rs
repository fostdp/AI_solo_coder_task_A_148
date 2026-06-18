use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorData {
    pub device_id: String,
    pub timestamp: DateTime<Utc>,
    pub cam_angle: f64,
    pub duitou_acceleration: f64,
    pub grain_reaction_force: f64,
    pub frame_vibration_x: f64,
    pub frame_vibration_y: f64,
    pub frame_vibration_z: f64,
    pub frame_vibration_total: f64,
    pub water_wheel_speed: f64,
    pub duitou_position: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicsResult {
    pub device_id: String,
    pub timestamp: DateTime<Utc>,
    pub cam_angle: f64,
    pub pounding_force: f64,
    pub impact_energy: f64,
    pub duitou_velocity: f64,
    pub duitou_displacement: f64,
    pub contact_time: f64,
    pub restitution_coefficient: f64,
    pub friction_force: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub device_id: String,
    pub timestamp: DateTime<Utc>,
    pub alert_type: String,
    pub alert_level: String,
    pub alert_message: String,
    pub alert_value: f64,
    pub threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub device_name: String,
    pub location: String,
    pub cam_base_radius: f64,
    pub cam_lift: f64,
    pub duitou_mass: f64,
    pub water_flow_rate: f64,
    pub frame_vibration_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationRequest {
    pub device_id: String,
    pub target_efficiency: f64,
    pub grain_type: String,
    pub constraints: OptimizationConstraints,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConstraints {
    pub max_cam_radius: f64,
    pub min_cam_radius: f64,
    pub max_lift: f64,
    pub max_pressure_angle: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    pub id: String,
    pub device_id: String,
    pub timestamp: DateTime<Utc>,
    pub cam_base_radius: f64,
    pub cam_lift: f64,
    pub cam_pressure_angle: f64,
    pub cam_profile_type: String,
    pub target_efficiency: f64,
    pub actual_efficiency: f64,
    pub average_pounding_force: f64,
    pub impact_energy_per_cycle: f64,
    pub husking_rate: f64,
    pub grain_breakage_rate: f64,
    pub cam_profile_points: Vec<CamPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CamPoint {
    pub angle: f64,
    pub radius: f64,
    pub lift: f64,
    pub velocity: f64,
    pub acceleration: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttSensorMessage {
    pub device_id: String,
    pub timestamp: i64,
    pub cam_angle: f64,
    pub duitou_acceleration: f64,
    pub grain_reaction_force: f64,
    pub frame_vibration_x: f64,
    pub frame_vibration_y: f64,
    pub frame_vibration_z: f64,
    pub water_wheel_speed: f64,
    pub duitou_position: f64,
}

impl From<MqttSensorMessage> for SensorData {
    fn from(msg: MqttSensorMessage) -> Self {
        let total_vib = (msg.frame_vibration_x.powi(2)
            + msg.frame_vibration_y.powi(2)
            + msg.frame_vibration_z.powi(2))
        .sqrt();

        SensorData {
            device_id: msg.device_id,
            timestamp: DateTime::from_timestamp_millis(msg.timestamp)
                .unwrap_or_else(|| Utc::now()),
            cam_angle: msg.cam_angle,
            duitou_acceleration: msg.duitou_acceleration,
            grain_reaction_force: msg.grain_reaction_force,
            frame_vibration_x: msg.frame_vibration_x,
            frame_vibration_y: msg.frame_vibration_y,
            frame_vibration_z: msg.frame_vibration_z,
            frame_vibration_total: total_vib,
            water_wheel_speed: msg.water_wheel_speed,
            duitou_position: msg.duitou_position,
        }
    }
}
