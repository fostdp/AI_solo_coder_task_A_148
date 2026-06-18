use crate::models::{DynamicsResult, SensorData, DeviceInfo, CamPoint};
use crate::config::DynamicsConfig;
use chrono::Utc;

pub struct PenaltyContactModel {
    stiffness: f64,
    damping: f64,
    penetration: f64,
    prev_penetration: f64,
    contact_radius: f64,
    damping_ratio: f64,
    penalty_exponent: f64,
    max_penetration: f64,
}

impl PenaltyContactModel {
    pub fn new(equivalent_radius: f64, config: &DynamicsConfig) -> Self {
        let equivalent_young = Self::calculate_equivalent_young_modulus(config);
        let stiffness = Self::calculate_hertz_stiffness(equivalent_young, equivalent_radius);
        let mass = 25.0;
        let natural_freq = (stiffness / mass).sqrt();
        let damping = 2.0 * config.damping_ratio * mass * natural_freq;

        PenaltyContactModel {
            stiffness,
            damping,
            penetration: 0.0,
            prev_penetration: 0.0,
            contact_radius: equivalent_radius,
            damping_ratio: config.damping_ratio,
            penalty_exponent: config.penalty_exponent,
            max_penetration: config.max_penetration_meters,
        }
    }

    fn calculate_equivalent_young_modulus(config: &DynamicsConfig) -> f64 {
        let grain_term = (1.0 - config.contact.grain_poisson_ratio.powi(2))
            / config.contact.grain_young_modulus_pa;
        let cam_term = (1.0 - config.contact.cam_poisson_ratio.powi(2))
            / config.contact.cam_young_modulus_pa;
        1.0 / (grain_term + cam_term)
    }

    fn calculate_hertz_stiffness(e_star: f64, r_eq: f64) -> f64 {
        (4.0 / 3.0) * e_star * r_eq.sqrt()
    }

    fn adaptive_stiffness(&mut self, penetration: f64, velocity: f64) -> f64 {
        let velocity_factor = 1.0 + velocity.abs().min(5.0) / 5.0 * 0.5;
        let penetration_factor = if penetration > self.max_penetration {
            let excess = penetration - self.max_penetration;
            1.0 + (excess / self.max_penetration).powi(2) * 10.0
        } else {
            1.0 + (penetration / self.max_penetration) * 0.3
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
        let normal_force = stiffness * penetration.powf(self.penalty_exponent);
        let damping_force = self.damping * effective_velocity.max(0.0);
        let total_normal = normal_force + damping_force;
        let max_allowable_force =
            self.stiffness * self.max_penetration.powf(self.penalty_exponent) * 20.0;

        (
            total_normal.min(max_allowable_force),
            normal_force,
            damping_force,
        )
    }

    pub fn calculate_contact_area(&self, penetration: f64) -> f64 {
        if penetration <= 0.0 { return 0.0; }
        std::f64::consts::PI * self.contact_radius * penetration
    }

    pub fn calculate_contact_stress(&self, force: f64, area: f64) -> f64 {
        if area <= 0.0 { return 0.0; }
        (6.0 * force * self.stiffness.powi(2)
            / (std::f64::consts::PI.powi(2) * self.contact_radius)).powf(1.0 / 3.0)
    }

    pub fn get_penetration(&self) -> f64 { self.penetration }
    pub fn get_stiffness(&self) -> f64 { self.stiffness }
    pub fn get_damping_ratio(&self) -> f64 { self.damping_ratio }
    pub fn get_penalty_exponent(&self) -> f64 { self.penalty_exponent }
    pub fn get_max_penetration(&self) -> f64 { self.max_penetration }
}

pub struct CamDynamicsSimulator {
    device: DeviceInfo,
    config: DynamicsConfig,
    restitution_coeff: f64,
    friction_coeff: f64,
    contact_model: PenaltyContactModel,
    last_lift: f64,
    last_timestamp: Option<chrono::DateTime<Utc>>,
}

impl CamDynamicsSimulator {
    pub fn new(device: DeviceInfo) -> Self {
        Self::new_with_config(device, &DynamicsConfig::load_default())
    }

    pub fn new_with_config(device: DeviceInfo, config: &DynamicsConfig) -> Self {
        let contact_radius = config.contact.duitou_contact_radius_meters;
        let contact_model = PenaltyContactModel::new(contact_radius, config);

        CamDynamicsSimulator {
            device,
            config: config.clone(),
            restitution_coeff: config.restitution_default,
            friction_coeff: config.friction_default,
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

        let impact_energy = self.calculate_impact_energy(
            velocity, displacement, contact_force, penetration);
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
        let max_p = self.contact_model.get_max_penetration();
        let expected_gap = self.device.cam_lift - theoretical_lift;
        let penetration = (expected_gap - actual_position).max(0.0);
        penetration.min(max_p * 2.0)
    }

    fn calculate_cam_lift(&self, angle: f64) -> f64 {
        let r0 = self.device.cam_base_radius;
        let _ = r0;
        let h = self.device.cam_lift;
        if angle < 0.0 { return 0.0; }

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
        let g = self.config.gravity;
        let inertia_force = mass * acceleration;
        let gravity_force = mass * g;

        let impulse_force = if velocity.abs() > 0.1 && contact_force < 10.0 {
            let impulse = mass * velocity.abs() * (1.0 + self.restitution_coeff);
            let impact_duration = 0.001 + (velocity.abs() * 0.001).min(0.01);
            impulse / impact_duration
        } else { 0.0 };

        let penalty_component = if contact_force > 0.0 { contact_force } else { 0.0 };
        let total_force = gravity_force + inertia_force + penalty_component
            + impulse_force - grain_reaction;
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
        let g = self.config.gravity;
        let kinetic_energy = 0.5 * mass * velocity.powi(2);
        let potential_energy = mass * g * displacement;
        let exp = self.contact_model.get_penalty_exponent();
        let dr = self.contact_model.get_damping_ratio();

        let strain_energy = if penetration > 0.0 {
            let stiffness = self.contact_model.get_stiffness();
            0.5 * stiffness * penetration.powf(exp + 1.0) / (exp + 1.0)
        } else { 0.0 };

        let damping_dissipated = contact_force * penetration.abs() * dr;
        (kinetic_energy + potential_energy + strain_energy - damping_dissipated).max(0.0)
    }

    fn calculate_contact_time(&self, velocity: f64, force: f64, penetration: f64) -> f64 {
        if force < 1.0 { return 0.0; }
        let mass = self.device.duitou_mass;
        let stiffness = self.contact_model.get_stiffness();

        if penetration > 0.0 {
            let hertz_time = 2.94
                * (mass.powi(2) / (stiffness.powi(2) * velocity.abs())).powf(1.0 / 5.0);
            hertz_time.max(0.0001)
        } else {
            let delta_v = velocity.abs() * (1.0 + self.restitution_coeff);
            if force > 0.0 { (mass * delta_v) / force } else { 0.0 }
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
                let a = lift * std::f64::consts::PI.powi(2)
                    * (std::f64::consts::PI * t).cos() / 2.0;
                (s, v, a)
            } else {
                let t = (angle - std::f64::consts::PI) / std::f64::consts::PI;
                let s = lift * (1.0 + (std::f64::consts::PI * t).cos()) / 2.0;
                let v = -lift * std::f64::consts::PI * (std::f64::consts::PI * t).sin() / 2.0;
                let a = -lift * std::f64::consts::PI.powi(2)
                    * (std::f64::consts::PI * t).cos() / 2.0;
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

pub fn calculate_husking_rate(impact_energy: f64, grain_type: &str, config: &DynamicsConfig) -> f64 {
    let optimal = match grain_type {
        "rice" => config.husking_optimal_energy.rice_joules,
        "millet" => config.husking_optimal_energy.millet_joules,
        "wheat" => config.husking_optimal_energy.wheat_joules,
        _ => return 0.6,
    };
    let (div, mult, min_val, max_val) = match grain_type {
        "rice" => (2.0, 0.8, 0.1, 0.98),
        "millet" => (1.5, 0.75, 0.15, 0.95),
        "wheat" => (2.5, 0.85, 0.1, 0.97),
        _ => (2.0, 0.8, 0.1, 0.98),
    };
    let efficiency = 1.0 - ((impact_energy - optimal).powi(2) / div).exp() * mult;
    efficiency.max(min_val).min(max_val)
}

pub fn calculate_grain_breakage_rate(
    impact_energy: f64,
    pounding_force: f64,
    config: &DynamicsConfig,
) -> f64 {
    let gb = &config.grain_breakage;
    let energy_factor = (impact_energy / gb.energy_reference_joules).min(1.0);
    let force_factor = (pounding_force / gb.force_reference_newtons).min(1.0);
    gb.base_breakage_rate + energy_factor * gb.energy_weight + force_factor * gb.force_weight
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
        let cfg = DynamicsConfig::load_default();
        let mut model = PenaltyContactModel::new(0.15, &cfg);
        let (force, elastic, damping) = model.calculate_contact_force(0.001, 1.0, 0.01);
        assert!(force > 0.0);
        assert!(elastic > 0.0);
        assert!(damping >= 0.0);
    }

    #[test]
    fn test_penalty_model_high_speed() {
        let cfg = DynamicsConfig::load_default();
        let mut m1 = PenaltyContactModel::new(0.15, &cfg);
        let _ = m1.calculate_contact_force(0.0005, 0.5, 0.01);
        let (f_low, _, _) = m1.calculate_contact_force(0.001, 0.5, 0.01);
        let mut m2 = PenaltyContactModel::new(0.15, &cfg);
        let _ = m2.calculate_contact_force(0.0005, 5.0, 0.01);
        let (f_high, _, _) = m2.calculate_contact_force(0.005, 5.0, 0.01);
        assert!(f_high > f_low);
    }

    #[test]
    fn test_no_penetration_no_force() {
        let cfg = DynamicsConfig::load_default();
        let mut model = PenaltyContactModel::new(0.15, &cfg);
        let (force, _, _) = model.calculate_contact_force(0.0, -1.0, 0.01);
        assert_eq!(force, 0.0);
    }

    #[test]
    fn test_simulate_basic() {
        let device = create_test_device();
        let cfg = DynamicsConfig::load_default();
        let mut simulator = CamDynamicsSimulator::new_with_config(device, &cfg);

        let sensor = SensorData {
            device_id: "test-001".to_string(),
            timestamp: Utc::now(),
            cam_angle: 90.0,
            duitou_acceleration: 5.0,
            grain_reaction_force: 100.0,
            frame_vibration_x: 0.1, frame_vibration_y: 0.2, frame_vibration_z: 0.3,
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
        let cfg = DynamicsConfig::load_default();
        let simulator = CamDynamicsSimulator::new_with_config(device, &cfg);
        let profile = simulator.generate_cam_profile(0.15, 0.12, 36);
        assert_eq!(profile.len(), 36);
        assert!(profile[0].lift >= 0.0);
    }
}
