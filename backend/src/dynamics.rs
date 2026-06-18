use crate::models::{DynamicsResult, SensorData, DeviceInfo, CamPoint};
use chrono::Utc;

const GRAVITY: f64 = 9.81;
const RESTITUTION_DEFAULT: f64 = 0.35;
const FRICTION_DEFAULT: f64 = 0.25;

pub struct CamDynamicsSimulator {
    device: DeviceInfo,
    restitution_coeff: f64,
    friction_coeff: f64,
}

impl CamDynamicsSimulator {
    pub fn new(device: DeviceInfo) -> Self {
        CamDynamicsSimulator {
            device,
            restitution_coeff: RESTITUTION_DEFAULT,
            friction_coeff: FRICTION_DEFAULT,
        }
    }

    pub fn simulate(&self, sensor: &SensorData) -> DynamicsResult {
        let cam_angle_rad = sensor.cam_angle.to_radians();

        let lift = self.calculate_cam_lift(cam_angle_rad);
        let velocity = self.calculate_duitou_velocity(cam_angle_rad, sensor.water_wheel_speed);
        let displacement = lift;

        let pounding_force = self.calculate_pounding_force(
            sensor.duitou_acceleration,
            velocity,
            sensor.grain_reaction_force,
        );

        let impact_energy = self.calculate_impact_energy(velocity, displacement);
        let contact_time = self.calculate_contact_time(velocity, pounding_force);
        let friction_force = self.calculate_friction_force(pounding_force);

        DynamicsResult {
            device_id: sensor.device_id.clone(),
            timestamp: sensor.timestamp,
            cam_angle: sensor.cam_angle,
            pounding_force,
            impact_energy,
            duitou_velocity: velocity,
            duitou_displacement: displacement,
            contact_time,
            restitution_coefficient: self.restitution_coeff,
            friction_force,
        }
    }

    fn calculate_cam_lift(&self, angle: f64) -> f64 {
        let r0 = self.device.cam_base_radius;
        let h = self.device.cam_lift;

        if angle < 0.0 {
            return 0.0;
        }

        let normalized_angle = (angle % (std::f64::consts::PI * 2.0)).abs();

        if normalized_angle < std::f64::consts::PI {
            let t = normalized_angle / std::f64::consts::PI;
            h * (1.0 - (std::f64::consts::PI * t).cos()) / 2.0
        } else {
            let t = (normalized_angle - std::f64::consts::PI) / std::f64::consts::PI;
            h * (1.0 + (std::f64::consts::PI * t).cos()) / 2.0
        }
    }

    fn calculate_duitou_velocity(&self, angle: f64, omega: f64) -> f64 {
        let h = self.device.cam_lift;
        let normalized_angle = (angle % (std::f64::consts::PI * 2.0)).abs();

        if normalized_angle < std::f64::consts::PI {
            let t = normalized_angle / std::f64::consts::PI;
            h * std::f64::consts::PI * (std::f64::consts::PI * t).sin() / 2.0 * omega
        } else {
            let t = (normalized_angle - std::f64::consts::PI) / std::f64::consts::PI;
            -h * std::f64::consts::PI * (std::f64::consts::PI * t).sin() / 2.0 * omega
        }
    }

    fn calculate_pounding_force(
        &self,
        acceleration: f64,
        velocity: f64,
        grain_reaction: f64,
    ) -> f64 {
        let mass = self.device.duitou_mass;
        let inertia_force = mass * acceleration;
        let gravity_force = mass * GRAVITY;

        let impact_force = if velocity.abs() > 0.1 {
            let impulse = mass * velocity.abs() * (1.0 + self.restitution_coeff);
            let impact_duration = 0.01;
            impulse / impact_duration
        } else {
            0.0
        };

        let total_force = gravity_force + inertia_force + impact_force - grain_reaction;

        total_force.max(0.0)
    }

    fn calculate_impact_energy(&self, velocity: f64, displacement: f64) -> f64 {
        let mass = self.device.duitou_mass;
        let kinetic_energy = 0.5 * mass * velocity.powi(2);
        let potential_energy = mass * GRAVITY * displacement;
        kinetic_energy + potential_energy
    }

    fn calculate_contact_time(&self, velocity: f64, force: f64) -> f64 {
        if force < 1.0 {
            return 0.0;
        }
        let mass = self.device.duitou_mass;
        let delta_v = velocity.abs() * (1.0 + self.restitution_coeff);
        if force > 0.0 {
            (mass * delta_v) / force
        } else {
            0.0
        }
    }

    fn calculate_friction_force(&self, normal_force: f64) -> f64 {
        normal_force * self.friction_coeff
    }

    pub fn generate_cam_profile(
        &self,
        base_radius: f64,
        lift: f64,
        num_points: usize,
    ) -> Vec<CamPoint> {
        let mut points = Vec::with_capacity(num_points);

        for i in 0..num_points {
            let angle = 2.0 * std::f64::consts::PI * i as f64 / num_points as f64;
            let angle_deg = angle.to_degrees();

            let lift_val = if angle < std::f64::consts::PI {
                let t = angle / std::f64::consts::PI;
                lift * (1.0 - (std::f64::consts::PI * t).cos()) / 2.0
            } else {
                let t = (angle - std::f64::consts::PI) / std::f64::consts::PI;
                lift * (1.0 + (std::f64::consts::PI * t).cos()) / 2.0
            };

            let velocity = if angle < std::f64::consts::PI {
                let t = angle / std::f64::consts::PI;
                lift * std::f64::consts::PI * (std::f64::consts::PI * t).sin() / 2.0
            } else {
                let t = (angle - std::f64::consts::PI) / std::f64::consts::PI;
                -lift * std::f64::consts::PI * (std::f64::consts::PI * t).sin() / 2.0
            };

            let acceleration = if angle < std::f64::consts::PI {
                let t = angle / std::f64::consts::PI;
                lift * std::f64::consts::PI.powi(2) * (std::f64::consts::PI * t).cos() / 2.0
            } else {
                let t = (angle - std::f64::consts::PI) / std::f64::consts::PI;
                -lift * std::f64::consts::PI.powi(2) * (std::f64::consts::PI * t).cos() / 2.0
            };

            points.push(CamPoint {
                angle: angle_deg,
                radius: base_radius + lift_val,
                lift: lift_val,
                velocity,
                acceleration,
            });
        }

        points
    }
}

pub fn calculate_husking_rate(impact_energy: f64, grain_type: &str) -> f64 {
    match grain_type {
        "rice" => {
            let optimal_energy = 0.5;
            let efficiency = 1.0 - ((impact_energy - optimal_energy).powi(2) / 2.0).exp() * 0.8;
            efficiency.max(0.1).min(0.98)
        }
        "millet" => {
            let optimal_energy = 0.3;
            let efficiency = 1.0 - ((impact_energy - optimal_energy).powi(2) / 1.5).exp() * 0.75;
            efficiency.max(0.15).min(0.95)
        }
        "wheat" => {
            let optimal_energy = 0.7;
            let efficiency = 1.0 - ((impact_energy - optimal_energy).powi(2) / 2.5).exp() * 0.85;
            efficiency.max(0.1).min(0.97)
        }
        _ => 0.6,
    }
}

pub fn calculate_grain_breakage_rate(impact_energy: f64, pounding_force: f64) -> f64 {
    let energy_factor = (impact_energy / 2.0).min(1.0);
    let force_factor = (pounding_force / 1000.0).min(1.0);
    0.05 + energy_factor * 0.25 + force_factor * 0.15
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_simulate_basic() {
        let device = create_test_device();
        let simulator = CamDynamicsSimulator::new(device);

        let sensor = SensorData {
            device_id: "test-001".to_string(),
            timestamp: Utc::now(),
            cam_angle: 90.0,
            duitou_acceleration: 5.0,
            grain_reaction_force: 100.0,
            frame_vibration_x: 0.1,
            frame_vibration_y: 0.2,
            frame_vibration_z: 0.3,
            frame_vibration_total: 0.37,
            water_wheel_speed: 3.14,
            duitou_position: 0.06,
        };

        let result = simulator.simulate(&sensor);
        assert!(result.pounding_force >= 0.0);
        assert!(result.impact_energy >= 0.0);
    }

    #[test]
    fn test_cam_profile_generation() {
        let device = create_test_device();
        let simulator = CamDynamicsSimulator::new(device);
        let profile = simulator.generate_cam_profile(0.15, 0.12, 36);
        assert_eq!(profile.len(), 36);
        assert!(profile[0].lift >= 0.0);
    }
}
