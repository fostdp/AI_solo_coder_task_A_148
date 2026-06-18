use crate::models::{
    OptimizationRequest, OptimizationResult, CamPoint, DeviceInfo, ToleranceReport,
};
use crate::dynamics::{calculate_husking_rate, calculate_grain_breakage_rate};
use crate::config::{OptimizationConfig, ToleranceConfig, DynamicsConfig};
use chrono::Utc;
use uuid::Uuid;

const GRAVITY: f64 = 9.81;
const MANUFACTURING_COST_BASE: f64 = 1000.0;

pub struct ToleranceAnalysis {
    dimensional_tolerance: f64,
    surface_roughness: f64,
    angular_tolerance: f64,
    min_curvature_radius: f64,
    jerk_limit: f64,
    manufacturing_cost_base: f64,
}

impl Default for ToleranceAnalysis {
    fn default() -> Self {
        let cfg = OptimizationConfig::load_default();
        Self::from_config(&cfg.tolerance)
    }
}

impl ToleranceAnalysis {
    pub fn from_config(cfg: &ToleranceConfig) -> Self {
        ToleranceAnalysis {
            dimensional_tolerance: cfg.dimensional_tolerance_meters,
            surface_roughness: cfg.surface_roughness_ra_meters,
            angular_tolerance: cfg.angular_tolerance_radians,
            min_curvature_radius: cfg.min_curvature_radius_meters,
            jerk_limit: cfg.jerk_limit_m_per_s3,
            manufacturing_cost_base: cfg.manufacturing_cost_base_cny,
        }
    }

    pub fn new(
        dimensional_tolerance: f64,
        surface_roughness: f64,
        angular_tolerance: f64,
        min_curvature_radius: f64,
    ) -> Self {
        ToleranceAnalysis {
            dimensional_tolerance,
            surface_roughness,
            angular_tolerance,
            min_curvature_radius,
            jerk_limit: 1000.0,
            manufacturing_cost_base: 1000.0,
        }
    }

    pub fn analyze(&self, profile: &[CamPoint]) -> ToleranceReport {
        let mut report = ToleranceReport {
            min_curvature: 0.0,
            lift_deviation: 0.0,
            pressure_angle_variation: 0.0,
            surface_sensitivity: 0.0,
            jerk: 0.0,
            overall_feasibility: 0.0,
            manufacturing_cost: 0.0,
            curvature_ok: false,
            lift_ok: false,
            pressure_angle_ok: false,
            jerk_ok: false,
            surface_ok: false,
        };

        report.min_curvature = self.calculate_min_curvature(profile);
        report.curvature_ok = report.min_curvature >= self.min_curvature_radius;

        report.lift_deviation = self.calculate_lift_tolerance_sensitivity(profile);
        report.lift_ok = report.lift_deviation <= self.dimensional_tolerance * 5.0;

        report.pressure_angle_variation = self.calculate_pressure_angle_variation(profile);
        report.pressure_angle_ok = report.pressure_angle_variation <= self.angular_tolerance;

        report.surface_sensitivity = self.calculate_surface_sensitivity(profile);
        report.surface_ok = report.surface_sensitivity <= self.surface_roughness * 100.0;

        report.jerk = self.calculate_max_jerk(profile);
        report.jerk_ok = report.jerk < self.jerk_limit;

        report.overall_feasibility =
            report.curvature_ok as u8 as f64 * 0.35 +
            report.lift_ok as u8 as f64 * 0.25 +
            report.pressure_angle_ok as u8 as f64 * 0.2 +
            report.jerk_ok as u8 as f64 * 0.1 +
            report.surface_ok as u8 as f64 * 0.1;

        report.manufacturing_cost = self.estimate_manufacturing_cost(&report, profile);

        report
    }

    fn calculate_min_curvature(&self, profile: &[CamPoint]) -> f64 {
        let n = profile.len();
        let mut min_r = f64::INFINITY;

        for i in 0..n {
            let i0 = (i + n - 1) % n;
            let i1 = i;
            let i2 = (i + 1) % n;

            let p0 = &profile[i0];
            let p1 = &profile[i1];
            let p2 = &profile[i2];

            let a0 = p0.angle.to_radians();
            let a1 = p1.angle.to_radians();
            let a2 = p2.angle.to_radians();

            let x0 = p0.radius * a0.cos();
            let y0 = p0.radius * a0.sin();
            let x1 = p1.radius * a1.cos();
            let y1 = p1.radius * a1.sin();
            let x2 = p2.radius * a2.cos();
            let y2 = p2.radius * a2.sin();

            let curvature = Self::curvature_from_points(x0, y0, x1, y1, x2, y2);
            if curvature > 0.0 {
                let r = 1.0 / curvature;
                if r < min_r {
                    min_r = r;
                }
            }
        }

        min_r
    }

    fn curvature_from_points(
        x0: f64, y0: f64,
        x1: f64, y1: f64,
        x2: f64, y2: f64,
    ) -> f64 {
        let dx1 = x1 - x0;
        let dy1 = y1 - y0;
        let dx2 = x2 - x1;
        let dy2 = y2 - y1;

        let cross = dx1 * dy2 - dy1 * dx2;
        let d1 = (dx1 * dx1 + dy1 * dy1).sqrt();
        let d2 = (dx2 * dx2 + dy2 * dy2).sqrt();
        let d12 = ((x2 - x0) * (x2 - x0) + (y2 - y0) * (y2 - y0)).sqrt();

        if d1 < 1e-10 || d2 < 1e-10 || d12 < 1e-10 {
            return 0.0;
        }

        2.0 * cross.abs() / (d1 * d2 * d12)
    }

    fn calculate_lift_tolerance_sensitivity(&self, profile: &[CamPoint]) -> f64 {
        if profile.len() < 3 {
            return 0.0;
        }

        let lifts: Vec<f64> = profile.iter().map(|p| p.lift).collect();
        let mean_lift: f64 = lifts.iter().sum::<f64>() / lifts.len() as f64;

        let variance: f64 = lifts
            .iter()
            .map(|l| (l - mean_lift).powi(2))
            .sum::<f64>() / lifts.len() as f64;

        variance.sqrt()
    }

    fn calculate_pressure_angle_variation(&self, profile: &[CamPoint]) -> f64 {
        if profile.len() < 2 {
            return 0.0;
        }

        let mut max_variation = 0.0;

        for i in 1..profile.len() {
            let _prev = &profile[i - 1];
            let curr = &profile[i];

            if curr.radius > 0.0 {
                let variation = (curr.velocity / curr.radius).abs();
                if variation > max_variation {
                    max_variation = variation;
                }
            }
        }

        max_variation
    }

    fn calculate_surface_sensitivity(&self, profile: &[CamPoint]) -> f64 {
        if profile.len() < 2 {
            return 0.0;
        }

        let mut total_variation = 0.0;

        for i in 1..profile.len() {
            let prev = &profile[i - 1];
            let curr = &profile[i];
            let d_angle = (curr.angle - prev.angle).to_radians();

            let dr = (curr.radius - prev.radius).abs();
            let arc_length = ((curr.radius + prev.radius) / 2.0) * d_angle.abs();

            if arc_length > 0.0 {
                total_variation += dr / arc_length;
            }
        }

        total_variation / profile.len() as f64
    }

    fn calculate_max_jerk(&self, profile: &[CamPoint]) -> f64 {
        if profile.len() < 3 {
            return 0.0;
        }

        let mut max_jerk = 0.0;

        for i in 2..profile.len() {
            let a0 = profile[i - 2].acceleration;
            let a1 = profile[i - 1].acceleration;
            let a2 = profile[i].acceleration;

            let t0 = profile[i - 2].angle.to_radians();
            let t1 = profile[i - 1].angle.to_radians();
            let t2 = profile[i].angle.to_radians();

            let dt1 = t1 - t0;
            let dt2 = t2 - t1;

            if dt1 > 0.0 && dt2 > 0.0 {
                let da1 = (a1 - a0) / dt1;
                let da2 = (a2 - a1) / dt2;
                let jerk = ((da2 - da1) / ((dt1 + dt2) / 2.0)).abs();
                if jerk > max_jerk {
                    max_jerk = jerk;
                }
            }
        }

        max_jerk
    }

    fn estimate_manufacturing_cost(
        &self,
        report: &ToleranceReport,
        profile: &[CamPoint],
    ) -> f64 {
        let mut cost = self.manufacturing_cost_base;

        if !report.curvature_ok {
            cost *= 1.5;
        }

        let tightness = (5e-5 / self.dimensional_tolerance.max(1e-9)).max(1.0);
        cost *= tightness;

        let roughness_factor = (1.6e-6 / self.surface_roughness.max(0.1e-6)).max(1.0);
        cost *= roughness_factor.sqrt();

        if !report.jerk_ok {
            cost *= 1.3;
        }

        let complexity = profile.len() as f64 / 360.0;
        cost *= 1.0 + complexity * 0.2;

        cost
    }
}

pub struct PoundingOptimizer {
    device: DeviceInfo,
    tolerance: ToleranceAnalysis,
    dynamics_config: DynamicsConfig,
}

impl PoundingOptimizer {
    pub fn new(device: DeviceInfo) -> Self {
        PoundingOptimizer {
            device,
            tolerance: ToleranceAnalysis::default(),
            dynamics_config: DynamicsConfig::load_default(),
        }
    }

    pub fn with_tolerance(device: DeviceInfo, tolerance: ToleranceAnalysis) -> Self {
        PoundingOptimizer {
            device,
            tolerance,
            dynamics_config: DynamicsConfig::load_default(),
        }
    }

    pub fn with_config(device: DeviceInfo, tolerance: ToleranceAnalysis, dynamics_config: DynamicsConfig) -> Self {
        PoundingOptimizer {
            device,
            tolerance,
            dynamics_config,
        }
    }

    pub fn optimize(&self, request: &OptimizationRequest) -> OptimizationResult {
        let constraints = &request.constraints;

        let mut best_score = 0.0;
        let mut best_params = (self.device.cam_base_radius, self.device.cam_lift, "cycloidal".to_string());
        let mut best_profile: Vec<CamPoint> = Vec::new();
        let mut best_tolerance_report = ToleranceReport::default();

        let profile_types = vec!["cycloidal", "harmonic", "trapezoidal", "polynomial"];

        let radius_step = ((constraints.max_cam_radius - constraints.min_cam_radius) / 10.0).max(0.005);
        let lift_step = (constraints.max_lift / 10.0).max(0.005);

        for profile_type in &profile_types {
            let mut r = constraints.min_cam_radius;
            while r <= constraints.max_cam_radius {
                let mut h = lift_step;
                while h <= constraints.max_lift {
                    if let Some(pressure_angle) = self.calculate_pressure_angle(r, h) {
                        if pressure_angle <= constraints.max_pressure_angle {
                            let profile = self.generate_profile(profile_type, r, h);
                            let tolerance_report = self.tolerance.analyze(&profile);

                            let efficiency = self.evaluate_efficiency(
                                &profile,
                                &request.grain_type,
                                r,
                                h,
                            );

                            let cost_factor =
                                (MANUFACTURING_COST_BASE / tolerance_report.manufacturing_cost)
                                    .max(0.3)
                                    .min(1.0);

                            let tolerance_factor = tolerance_report.overall_feasibility.max(0.3);
                            let score = efficiency * cost_factor * tolerance_factor * 0.7
                                + efficiency * 0.3;

                            if score > best_score {
                                best_score = score;
                                best_params = (r, h, profile_type.to_string());
                                best_profile = profile.clone();
                                best_tolerance_report = tolerance_report.clone();
                            }
                        }
                    }
                    h += lift_step;
                }
                r += radius_step;
            }
        }

        let avg_force = self.calculate_average_pounding_force(&best_profile, best_params.0, best_params.1);
        let impact_energy = self.calculate_impact_energy_per_cycle(best_params.1);
        let husking_rate = calculate_husking_rate(impact_energy, &request.grain_type, &self.dynamics_config);
        let breakage_rate = calculate_grain_breakage_rate(impact_energy, avg_force, &self.dynamics_config);
        let pressure_angle = self.calculate_pressure_angle(best_params.0, best_params.1)
            .unwrap_or(0.0);

        OptimizationResult {
            optimization_id: Uuid::new_v4().to_string(),
            device_id: request.device_id.clone(),
            timestamp: Utc::now(),
            grain_type: request.grain_type.clone(),
            base_radius: best_params.0,
            lift: best_params.1,
            cam_profile_type: best_params.2,
            overall_efficiency: best_score,
            husking_rate,
            breakage_rate,
            pounding_force: avg_force,
            impact_energy,
            manufacturing_cost: best_tolerance_report.manufacturing_cost,
            cam_profile: best_profile,
            tolerance_report: Some(best_tolerance_report),
            optimization_score: best_score,
        }
    }

    fn generate_profile(&self, profile_type: &str, base_radius: f64, lift: f64) -> Vec<CamPoint> {
        let num_points = 360;
        let mut points = Vec::with_capacity(num_points);

        for i in 0..num_points {
            let angle_rad = 2.0 * std::f64::consts::PI * i as f64 / num_points as f64;
            let angle_deg = angle_rad.to_degrees();

            let (lift_val, velocity, acceleration) = match profile_type {
                "cycloidal" => self.cycloidal_motion(angle_rad, lift),
                "harmonic" => self.harmonic_motion(angle_rad, lift),
                "trapezoidal" => self.trapezoidal_motion(angle_rad, lift),
                "polynomial" => self.polynomial_motion(angle_rad, lift),
                _ => self.harmonic_motion(angle_rad, lift),
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

    fn cycloidal_motion(&self, angle: f64, total_lift: f64) -> (f64, f64, f64) {
        let pi = std::f64::consts::PI;

        if angle < pi {
            let t = angle / pi;
            let s = total_lift * (t - (2.0 * pi * t).sin() / (2.0 * pi));
            let v = total_lift * (1.0 - (2.0 * pi * t).cos()) / pi;
            let a = 2.0 * total_lift * (2.0 * pi * t).sin();
            (s, v, a)
        } else {
            let t = (angle - pi) / pi;
            let s = total_lift * (1.0 - t + (2.0 * pi * t).sin() / (2.0 * pi));
            let v = -total_lift * (1.0 - (2.0 * pi * t).cos()) / pi;
            let a = -2.0 * total_lift * (2.0 * pi * t).sin();
            (s, v, a)
        }
    }

    fn harmonic_motion(&self, angle: f64, total_lift: f64) -> (f64, f64, f64) {
        let pi = std::f64::consts::PI;

        if angle < pi {
            let t = angle / pi;
            let s = total_lift * (1.0 - (pi * t).cos()) / 2.0;
            let v = total_lift * pi * (pi * t).sin() / 2.0;
            let a = total_lift * pi * pi * (pi * t).cos() / 2.0;
            (s, v, a)
        } else {
            let t = (angle - pi) / pi;
            let s = total_lift * (1.0 + (pi * t).cos()) / 2.0;
            let v = -total_lift * pi * (pi * t).sin() / 2.0;
            let a = -total_lift * pi * pi * (pi * t).cos() / 2.0;
            (s, v, a)
        }
    }

    fn trapezoidal_motion(&self, angle: f64, total_lift: f64) -> (f64, f64, f64) {
        let pi = std::f64::consts::PI;

        if angle < pi {
            let t = angle / pi;
            let t1 = 0.2;
            let t2 = 0.8;
            let v_max = total_lift / (pi * (t2 - t1 + t1));

            if t < t1 {
                let a_val = v_max / (t1 * pi);
                let s_val = 0.5 * a_val * (t * pi).powi(2);
                let v_val = a_val * t * pi;
                (s_val, v_val, a_val)
            } else if t < t2 {
                let s_val = 0.5 * v_max * t1 * pi + v_max * (t - t1) * pi;
                (s_val, v_max, 0.0)
            } else {
                let a_val = -v_max / ((1.0 - t2) * pi);
                let dt = (t - t2) * pi;
                let s_val = total_lift - 0.5 * (-a_val) * dt.powi(2);
                let v_val = v_max + a_val * dt;
                (s_val, v_val, a_val)
            }
        } else {
            let t = (angle - pi) / pi;
            let t1 = 0.2;
            let t2 = 0.8;
            let v_max = -total_lift / (pi * (t2 - t1 + t1));

            if t < t1 {
                let a_val = v_max / (t1 * pi);
                let s_val = total_lift + 0.5 * a_val * (t * pi).powi(2);
                let v_val = a_val * t * pi;
                (s_val, v_val, a_val)
            } else if t < t2 {
                let s_val = total_lift + 0.5 * v_max * t1 * pi + v_max * (t - t1) * pi;
                (s_val, v_max, 0.0)
            } else {
                let a_val = -v_max / ((1.0 - t2) * pi);
                let dt = (t - t2) * pi;
                let s_val = 0.0 - 0.5 * (-a_val) * dt.powi(2);
                let v_val = v_max + a_val * dt;
                (s_val, v_val, a_val)
            }
        }
    }

    fn polynomial_motion(&self, angle: f64, total_lift: f64) -> (f64, f64, f64) {
        let pi = std::f64::consts::PI;

        if angle < pi {
            let t = angle / pi;
            let s = total_lift * (10.0 * t.powi(3) - 15.0 * t.powi(4) + 6.0 * t.powi(5));
            let v = total_lift / pi * (30.0 * t.powi(2) - 60.0 * t.powi(3) + 30.0 * t.powi(4));
            let a = total_lift / (pi * pi) * (60.0 * t - 180.0 * t.powi(2) + 120.0 * t.powi(3));
            (s, v, a)
        } else {
            let t = (angle - pi) / pi;
            let s = total_lift * (1.0 - 10.0 * t.powi(3) + 15.0 * t.powi(4) - 6.0 * t.powi(5));
            let v = -total_lift / pi * (30.0 * t.powi(2) - 60.0 * t.powi(3) + 30.0 * t.powi(4));
            let a = -total_lift / (pi * pi) * (60.0 * t - 180.0 * t.powi(2) + 120.0 * t.powi(3));
            (s, v, a)
        }
    }

    fn evaluate_efficiency(
        &self,
        profile: &[CamPoint],
        grain_type: &str,
        base_radius: f64,
        lift: f64,
    ) -> f64 {
        let impact_energy = self.calculate_impact_energy_per_cycle(lift);
        let avg_force = self.calculate_average_pounding_force(profile, base_radius, lift);

        let husking_rate = calculate_husking_rate(impact_energy, grain_type, &self.dynamics_config);
        let breakage_rate = calculate_grain_breakage_rate(impact_energy, avg_force, &self.dynamics_config);

        let net_efficiency = husking_rate * (1.0 - breakage_rate);

        let pressure_angle = self.calculate_pressure_angle(base_radius, lift)
            .unwrap_or(std::f64::MAX);
        let pressure_penalty = if pressure_angle > 30.0_f64.to_radians() {
            (30.0_f64.to_radians() / pressure_angle).powi(2)
        } else {
            1.0
        };

        net_efficiency * pressure_penalty
    }

    fn calculate_pressure_angle(&self, base_radius: f64, lift: f64) -> Option<f64> {
        if base_radius <= 0.0 {
            return None;
        }

        let max_slope = std::f64::consts::PI * lift / 2.0;
        let pressure_angle = (max_slope / base_radius).atan();

        Some(pressure_angle)
    }

    fn calculate_average_pounding_force(
        &self,
        profile: &[CamPoint],
        _base_radius: f64,
        _lift: f64,
    ) -> f64 {
        let mass = self.device.duitou_mass;

        let max_accel = profile
            .iter()
            .map(|p| p.acceleration.abs())
            .fold(0.0, f64::max);

        let inertia_force = mass * max_accel;
        let gravity_force = mass * GRAVITY;

        gravity_force + inertia_force
    }

    fn calculate_impact_energy_per_cycle(&self, lift: f64) -> f64 {
        let mass = self.device.duitou_mass;
        let velocity = (2.0 * GRAVITY * lift).sqrt();
        0.5 * mass * velocity.powi(2)
    }
}

pub fn calculate_efficiency_score(husking_rate: f64, breakage_rate: f64) -> f64 {
    husking_rate * 0.7 + (1.0 - breakage_rate) * 0.3
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
    fn test_tolerance_analysis() {
        let device = create_test_device();
        let optimizer = PoundingOptimizer::new(device);
        let profile = optimizer.generate_profile("cycloidal", 0.15, 0.12);

        let tolerance = ToleranceAnalysis::default();
        let report = tolerance.analyze(&profile);

        assert!(report.min_curvature > 0.0);
        assert!(report.overall_feasibility >= 0.0);
        assert!(report.overall_feasibility <= 1.0);
        assert!(report.manufacturing_cost > 0.0);
    }

    #[test]
    fn test_curvature_calculation() {
        let tolerance = ToleranceAnalysis::default();
        let c = ToleranceAnalysis::curvature_from_points(
            -1.0, 0.0,
            0.0, 1.0,
            1.0, 0.0,
        );
        assert!(c > 0.0);
    }

    #[test]
    fn test_optimization() {
        let device = create_test_device();
        let optimizer = PoundingOptimizer::new(device);

        let request = OptimizationRequest {
            device_id: "test-001".to_string(),
            target_efficiency: 0.85,
            grain_type: "rice".to_string(),
            constraints: crate::models::OptimizationConstraints {
                max_cam_radius: 0.25,
                min_cam_radius: 0.1,
                max_lift: 0.2,
                max_pressure_angle: 45.0_f64.to_radians(),
            },
        };

        let result = optimizer.optimize(&request);
        assert!(result.overall_efficiency > 0.0);
        assert!(result.overall_efficiency <= 1.0);
        assert!(!result.cam_profile.is_empty());
    }

    #[test]
    fn test_profile_generation() {
        let device = create_test_device();
        let optimizer = PoundingOptimizer::new(device);

        let profile = optimizer.generate_profile("cycloidal", 0.15, 0.12);
        assert_eq!(profile.len(), 360);

        let max_lift = profile.iter().map(|p| p.lift).fold(0.0, f64::max);
        assert!((max_lift - 0.12).abs() < 0.001);
    }
}
