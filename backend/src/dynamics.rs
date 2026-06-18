use crate::models::{DynamicsResult, SensorData, DeviceInfo, CamPoint};
use chrono::Utc;

const GRAVITY: f64 = 9.81;
const RESTITUTION_DEFAULT: f64 = 0.35;
const FRICTION_DEFAULT: f64 = 0.25;

const HERTZ_STIFFNESS_BASE: f64 = 5.0e6;
const DAMPING_RATIO: f64 = 0.3;
const MAX_PENETRATION: f64 = 0.005;
const PENALTY_EXPONENT: f64 = 1.5;

const GRAIN_YOUNG_MODULUS: f64 = 1.5e9;
const GRAIN_POISSON_RATIO: f64 = 0.3;
const CAM_YOUNG_MODULUS: f64 = 2.0e11;
const CAM_POISSON_RATIO: f64 = 0.3;

pub struct PenaltyContactModel {
    stiffness: f64,
    damping: f64,
    penetration: f64,
    prev_penetration: f64,
    contact_radius: f64,
}

impl PenaltyContactModel {
    pub fn new(equivalent_radius: f64) -> Self {
        let equivalent_young = Self::calculate_equivalent_young_modulus();
        let stiffness = Self::calculate_hertz_stiffness(equivalent_young, equivalent_radius);
        let mass = 25.0;
        let natural_freq = (stiffness / mass).sqrt();
        let damping = 2.0 * DAMPING_RATIO * mass * natural_freq;

        PenaltyContactModel {
            stiffness,
            damping,
            penetration: 0.0,
            prev_penetration: 0.0,
            contact_radius: equivalent_radius,
        }
    }

    fn calculate_equivalent_young_modulus() -> f64 {
        let grain_term = (1.0 - GRAIN_POISSON_RATIO.powi(2)) / GRAIN_YOUNG_MODULUS;
        let cam_term = (1.0 - CAM_POISSON_RATIO.powi(2)) / CAM_YOUNG_MODULUS;
        1.0 / (grain_term + cam_term)
    }

    fn calculate_hertz_stiffness(e_star: f64, r_eq: f64) -> f64 {
        (4.0 / 3.0) * e_star * r_eq.sqrt()
    }

    fn adaptive_stiffness(&mut self, penetration: f64, velocity: f64) -> f64 {
        let velocity_factor = 1.0 + velocity.abs().min(5.0) / 5.0 * 0.5;
        let penetration_factor = if penetration > MAX_PENETRATION {
            let excess = penetration - MAX_PENETRATION;
            1.0 + (excess / MAX_PENETRATION).powi(2) * 10.0
        } else {
            1.0 + (penetration / MAX_PENETRATION) * 0.3
        };
        self.stiffness * velocity_factor * penetration_factor
    }

    pub fn calculate_contact_force(
        &mut self,
        penetration: f64,
        penetration_velocity: f64,
        dt: f64,
    ) -> (f64, f64, f64) {
        if penetration <= 0.0 {
            self.prev_penetration = self.penetration;
            self.penetration = 0.0;
            return (0.0, 0.0, 0.0);
        }

        let effective_velocity = if dt > 0.0 {
            (penetration - self.prev_penetration) / dt
        } else {
            penetration_velocity
        };

        self.prev_penetration = self.penetration;
        self.penetration = penetration;

        let stiffness = self.adaptive_stiffness(penetration, effective_velocity);
        let normal_force = stiffness * penetration.powf(PENALTY_EXPONENT);

        let damping_force = self.damping * effective_velocity.max(0.0);

        let total_normal = normal_force + damping_force;
        let max_allowable_force = self.stiffness * MAX_PENETRATION.powf(PENALTY_EXPONENT) * 20.0;

        (
            total_normal.min(max_allowable_force),
            normal_force,
            damping_force,
        )
    }

    pub fn calculate_contact_area(&self, penetration: f64) -> f64 {
        if penetration <= 0.0 {
            return 0.0;
        }
        std::f64::consts::PI * self.contact_radius * penetration
    }

    pub fn calculate_contact_stress(&self, force: f64, area: f64) -> f64 {
        if area <= 0.0 {
            return 0.0;
        }
        let hertz_stress = (6.0 * force * self.stiffness.powi(2) / (std::f64::consts::PI.powi(2) * self.contact_radius)).powf(1.0 / 3.0);
        hertz_stress
    }

    pub fn get_penetration(&self) -> f64 {
        self.penetration
    }

    pub fn get_stiffness(&self) -> f64 {
        self.stiffness
    }
}

pub struct CamDynamicsSimulator {
    device: DeviceInfo,
    restitution_coeff: f64,
    friction_coeff: f64,
    contact_model: PenaltyContactModel,
    last_lift: f64,
    last_timestamp: Option<chrono::DateTime<Utc>>,
}

impl CamDynamicsSimulator {
    pub fn new(device: DeviceInfo) -> Self {
        let duitou_radius = 0.15;
        let contact_model = PenaltyContactModel::new(duitou_radius);

        CamDynamicsSimulator {
            device,
            restitution_coeff: RESTITUTION_DEFAULT,
            friction_coeff: FRICTION_DEFAULT,
            contact_model,
            last_lift: 0.0,
            last_timestamp: None,
        }
    }

    pub fn simulate(&mut self, sensor: &SensorData) -> DynamicsResult {
        let cam_angle_rad = sensor.cam_angle.to_radians();

        let lift = self.calculate_cam_lift(cam_angle_rad);
        let velocity = self.calculate_duitou_velocity(cam_angle_rad, sensor.water_wheel_speed);
        let displacement = lift;

        let dt = match self.last_timestamp {
            Some(last) => (sensor.timestamp - last).num_milliseconds() as f64 / 1000.0,
            None => 0.016,
        };
        self.last_timestamp = Some(sensor.timestamp);

        let penetration = self.calculate_penetration(lift, sensor.duitou_position);
        let penetration_velocity = velocity;

        let (contact_force, elastic_force, damping_force) =
            self.contact_model.calculate_contact_force(penetration, penetration_velocity, dt);

        let pounding_force = self.calculate_pounding_force(
            sensor.duitou_acceleration,
            velocity,
            sensor.grain_reaction_force,
            contact_force,
            elastic_force,
            damping_force,
        );

        let impact_energy = self.calculate_impact_energy(velocity, displacement, contact_force, penetration);
        let contact_time = self.calculate_contact_time(velocity, pounding_force, penetration);
        let friction_force = self.calculate_friction_force(pounding_force);

        self.last_lift = lift;

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

    fn calculate_penetration(&self, theoretical_lift: f64, actual_position: f64) -> f64 {
        let expected_gap = self.device.cam_lift - theoretical_lift;
        let penetration = (expected_gap - actual_position).max(0.0);
        penetration.min(MAX_PENETRATION * 2.0)
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
        contact_force: f64,
        _elastic_force: f64,
        _damping_force: f64,
    ) -> f64 {
        let mass = self.device.duitou_mass;
        let inertia_force = mass * acceleration;
        let gravity_force = mass * GRAVITY;

        let impulse_force = if velocity.abs() > 0.1 && contact_force < 10.0 {
            let impulse = mass * velocity.abs() * (1.0 + self.restitution_coeff);
            let impact_duration = 0.001 + (velocity.abs() * 0.001).min(0.01);
            impulse / impact_duration
        } else {
            0.0
        };

        let penalty_component = if contact_force > 0.0 {
            contact_force
        } else {
            0.0
        };

        let total_force = gravity_force + inertia_force + penalty_component + impulse_force - grain_reaction;

        total_force.max(0.0)
    }

    fn calculate_impact_energy(
        &self,
        velocity: f64,
        displacement: f64,
        contact_force: f64,
        penetration: f64,
    ) -> f64 {
        let mass = self.device.duitou_mass;
        let kinetic_energy = 0.5 * mass * velocity.powi(2);
        let potential_energy = mass * GRAVITY * displacement;

        let strain_energy = if penetration > 0.0 {
            let stiffness = self.contact_model.get_stiffness();
            0.5 * stiffness * penetration.powf(PENALTY_EXPONENT + 1.0) / (PENALTY_EXPONENT + 1.0)
        } else {
            0.0
        };

        let damping_dissipated = contact_force * penetration.abs() * DAMPING_RATIO;

        (kinetic_energy + potential_energy + strain_energy - damping_dissipated).max(0.0)
    }

    fn calculate_contact_time(&self, velocity: f64, force: f64, penetration: f64) -> f64 {
        if force < 1.0 {
            return 0.0;
        }

        let mass = self.device.duitou_mass;
        let stiffness = self.contact_model.get_stiffness();

        if penetration > 0.0 {
            let hertz_time = 2.94 * (mass.powi(2) / (stiffness.powi(2) * velocity.abs())).powf(1.0 / 5.0);
            hertz_time.max(0.0001)
        } else {
            let delta_v = velocity.abs() * (1.0 + self.restitution_coeff);
            if force > 0.0 {
                (mass * delta_v) / force
            } else {
                0.0
            }
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

            let (lift_val, velocity, acceleration) = if angle < std::f64::consts::PI {
                let t = angle / std::f64::consts::PI;
                let s = lift * (1.0 - (std::f64::consts::PI * t).cos()) / 2.0;
                let v = lift * std::f64::consts::PI * (std::f64::consts::PI * t).sin() / 2.0;
                let a = lift * std::f64::consts::PI.powi(2) * (std::f64::consts::PI * t).cos() / 2.0;
                (s, v, a)
            } else {
                let t = (angle - std::f64::consts::PI) / std::f64::consts::PI;
                let s = lift * (1.0 + (std::f64::consts::PI * t).cos()) / 2.0;
                let v = -lift * std::f64::consts::PI * (std::f64::consts::PI * t).sin() / 2.0;
                let a = -lift * std::f64::consts::PI.powi(2) * (std::f64::consts::PI * t).cos() / 2.0;
                (s, v, a)
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
    fn test_penalty_model_basic() {
        let mut model = PenaltyContactModel::new(0.15);
        let (force, elastic, damping) = model.calculate_contact_force(0.001, 1.0, 0.01);
        assert!(force > 0.0);
        assert!(elastic > 0.0);
        assert!(damping >= 0.0);
    }

    #[test]
    fn test_penalty_model_high_speed() {
        let mut model1 = PenaltyContactModel::new(0.15);
        let (force_low, _, _) = model1.calculate_contact_force(0.001, 0.5, 0.01);

        let mut model2 = PenaltyContactModel::new(0.15);
        let (force_high, _, _) = model2.calculate_contact_force(0.001, 5.0, 0.01);

        assert!(force_high > force_low);
    }

    #[test]
    fn test_no_penetration_no_force() {
        let mut model = PenaltyContactModel::new(0.15);
        let (force, _, _) = model.calculate_contact_force(0.0, -1.0, 0.01);
        assert_eq!(force, 0.0);
    }

    #[test]
    fn test_simulate_basic() {
        let device = create_test_device();
        let mut simulator = CamDynamicsSimulator::new(device);

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
