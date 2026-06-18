use crate::models::{SensorData, Alert, DeviceInfo};
use chrono::Utc;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

impl AlertLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertLevel::Info => "info",
            AlertLevel::Warning => "warning",
            AlertLevel::Critical => "critical",
        }
    }
}

pub struct AlertDetector {
    vibration_threshold: f64,
    acceleration_threshold: f64,
    stall_speed_threshold: f64,
}

impl AlertDetector {
    pub fn new(device: &DeviceInfo) -> Self {
        AlertDetector {
            vibration_threshold: device.frame_vibration_threshold,
            acceleration_threshold: 50.0,
            stall_speed_threshold: 0.1,
        }
    }

    pub fn detect(&self, sensor: &SensorData) -> Vec<Alert> {
        let mut alerts = Vec::new();

        if let Some(alert) = self.check_vibration(sensor) {
            alerts.push(alert);
        }

        if let Some(alert) = self.check_acceleration(sensor) {
            alerts.push(alert);
        }

        if let Some(alert) = self.check_stall(sensor) {
            alerts.push(alert);
        }

        alerts
    }

    fn check_vibration(&self, sensor: &SensorData) -> Option<Alert> {
        let vib = sensor.frame_vibration_total;

        if vib <= self.vibration_threshold {
            return None;
        }

        let (level, message) = if vib > self.vibration_threshold * 1.6 {
            (
                AlertLevel::Critical,
                "机架振动严重超限，请立即停机检查！可能存在机构损坏或基础松动。".to_string(),
            )
        } else if vib > self.vibration_threshold * 1.2 {
            (
                AlertLevel::Warning,
                "机架振动超过预警阈值，请检查设备运行状态。".to_string(),
            )
        } else {
            return None;
        };

        Some(Alert {
            id: Uuid::new_v4().to_string(),
            device_id: sensor.device_id.clone(),
            timestamp: Utc::now(),
            alert_type: "frame_vibration".to_string(),
            alert_level: level.as_str().to_string(),
            alert_message: message,
            alert_value: vib,
            threshold: self.vibration_threshold,
        })
    }

    fn check_acceleration(&self, sensor: &SensorData) -> Option<Alert> {
        let acc = sensor.duitou_acceleration.abs();

        if acc <= self.acceleration_threshold {
            return None;
        }

        let (level, message) = if acc > self.acceleration_threshold * 2.0 {
            (
                AlertLevel::Critical,
                "碓头加速度异常，可能发生碰撞或卡死！".to_string(),
            )
        } else {
            (
                AlertLevel::Warning,
                "碓头加速度偏高，请检查凸轮机构状态。".to_string(),
            )
        };

        Some(Alert {
            id: Uuid::new_v4().to_string(),
            device_id: sensor.device_id.clone(),
            timestamp: Utc::now(),
            alert_type: "duitou_acceleration".to_string(),
            alert_level: level.as_str().to_string(),
            alert_message: message,
            alert_value: acc,
            threshold: self.acceleration_threshold,
        })
    }

    fn check_stall(&self, sensor: &SensorData) -> Option<Alert> {
        if sensor.water_wheel_speed >= self.stall_speed_threshold {
            return None;
        }

        if sensor.cam_angle < 10.0 || sensor.cam_angle > 350.0 {
            return None;
        }

        Some(Alert {
            id: Uuid::new_v4().to_string(),
            device_id: sensor.device_id.clone(),
            timestamp: Utc::now(),
            alert_type: "duitou_stall".to_string(),
            alert_level: AlertLevel::Critical.as_str().to_string(),
            alert_message: "碓头可能卡死！水轮转速过低且凸轮不在初始位置。".to_string(),
            alert_value: sensor.water_wheel_speed,
            threshold: self.stall_speed_threshold,
        })
    }
}

pub fn alert_level_from_str(level: &str) -> AlertLevel {
    match level.to_lowercase().as_str() {
        "critical" => AlertLevel::Critical,
        "warning" => AlertLevel::Warning,
        _ => AlertLevel::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_device() -> DeviceInfo {
        DeviceInfo {
            device_id: "test-001".to_string(),
            device_name: "Test".to_string(),
            location: "Test".to_string(),
            cam_base_radius: 0.15,
            cam_lift: 0.12,
            duitou_mass: 25.0,
            water_flow_rate: 0.05,
            frame_vibration_threshold: 5.0,
        }
    }

    #[test]
    fn test_vibration_alert() {
        let device = create_test_device();
        let detector = AlertDetector::new(&device);

        let sensor = SensorData {
            device_id: "test-001".to_string(),
            timestamp: Utc::now(),
            cam_angle: 90.0,
            duitou_acceleration: 5.0,
            grain_reaction_force: 100.0,
            frame_vibration_x: 4.0,
            frame_vibration_y: 3.0,
            frame_vibration_z: 0.0,
            frame_vibration_total: 5.0,
            water_wheel_speed: 3.0,
            duitou_position: 0.06,
        };

        let alerts = detector.detect(&sensor);
        assert!(alerts.is_empty());

        let sensor = SensorData {
            frame_vibration_total: 7.0,
            ..sensor
        };

        let alerts = detector.detect(&sensor);
        assert!(!alerts.is_empty());
        assert_eq!(alerts[0].alert_type, "frame_vibration");
    }

    #[test]
    fn test_stall_detection() {
        let device = create_test_device();
        let detector = AlertDetector::new(&device);

        let sensor = SensorData {
            device_id: "test-001".to_string(),
            timestamp: Utc::now(),
            cam_angle: 90.0,
            duitou_acceleration: 0.0,
            grain_reaction_force: 0.0,
            frame_vibration_x: 0.0,
            frame_vibration_y: 0.0,
            frame_vibration_z: 0.0,
            frame_vibration_total: 0.0,
            water_wheel_speed: 0.01,
            duitou_position: 0.06,
        };

        let alerts = detector.detect(&sensor);
        let has_stall = alerts.iter().any(|a| a.alert_type == "duitou_stall");
        assert!(has_stall);
    }
}
