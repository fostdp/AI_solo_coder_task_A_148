use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DynamicsConfig {
    pub gravity: f64,
    pub restitution_default: f64,
    pub friction_default: f64,
    pub hertz_stiffness_base: f64,
    pub damping_ratio: f64,
    pub max_penetration_meters: f64,
    pub penalty_exponent: f64,
    pub contact: ContactMaterialConfig,
    pub grain_breakage: GrainBreakageConfig,
    pub husking_optimal_energy: HuskingOptimalEnergy,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContactMaterialConfig {
    pub grain_young_modulus_pa: f64,
    pub grain_poisson_ratio: f64,
    pub cam_young_modulus_pa: f64,
    pub cam_poisson_ratio: f64,
    pub duitou_contact_radius_meters: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GrainBreakageConfig {
    pub base_breakage_rate: f64,
    pub energy_weight: f64,
    pub force_weight: f64,
    pub energy_reference_joules: f64,
    pub force_reference_newtons: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HuskingOptimalEnergy {
    pub rice_joules: f64,
    pub millet_joules: f64,
    pub wheat_joules: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OptimizationConfig {
    pub tolerance: ToleranceConfig,
    pub search_grid: SearchGridConfig,
    pub scoring_weights: ScoringWeightsConfig,
    pub pressure_angle_penalty: PressureAnglePenaltyConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToleranceConfig {
    pub dimensional_tolerance_meters: f64,
    pub surface_roughness_ra_meters: f64,
    pub angular_tolerance_radians: f64,
    pub min_curvature_radius_meters: f64,
    pub manufacturing_cost_base_cny: f64,
    pub feasibility_threshold: f64,
    pub jerk_limit_m_per_s3: f64,
    pub weights: ToleranceWeights,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToleranceWeights {
    pub curvature: f64,
    pub lift_tolerance: f64,
    pub pressure_angle: f64,
    pub jerk: f64,
    pub surface: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SearchGridConfig {
    pub radius_steps: usize,
    pub lift_steps: usize,
    pub profile_types: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScoringWeightsConfig {
    pub efficiency_pure: f64,
    pub efficiency_with_constraints: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PressureAnglePenaltyConfig {
    pub threshold_degrees: f64,
    pub decay_exponent: f64,
}

impl DynamicsConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let cfg: DynamicsConfig = serde_json::from_str(&content)?;
        Ok(cfg)
    }

    pub fn load_default() -> Self {
        Self::load_from_file("config/dynamics_config.json")
            .unwrap_or_else(|_| Self::default_config())
    }

    fn default_config() -> Self {
        DynamicsConfig {
            gravity: 9.81,
            restitution_default: 0.35,
            friction_default: 0.25,
            hertz_stiffness_base: 5.0e6,
            damping_ratio: 0.3,
            max_penetration_meters: 0.005,
            penalty_exponent: 1.5,
            contact: ContactMaterialConfig {
                grain_young_modulus_pa: 1.5e9,
                grain_poisson_ratio: 0.3,
                cam_young_modulus_pa: 2.0e11,
                cam_poisson_ratio: 0.3,
                duitou_contact_radius_meters: 0.15,
            },
            grain_breakage: GrainBreakageConfig {
                base_breakage_rate: 0.05,
                energy_weight: 0.25,
                force_weight: 0.15,
                energy_reference_joules: 2.0,
                force_reference_newtons: 1000.0,
            },
            husking_optimal_energy: HuskingOptimalEnergy {
                rice_joules: 0.5,
                millet_joules: 0.3,
                wheat_joules: 0.7,
            },
        }
    }
}

impl OptimizationConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let cfg: OptimizationConfig = serde_json::from_str(&content)?;
        Ok(cfg)
    }

    pub fn load_default() -> Self {
        Self::load_from_file("config/optimization_config.json")
            .unwrap_or_else(|_| Self::default_config())
    }

    fn default_config() -> Self {
        OptimizationConfig {
            tolerance: ToleranceConfig {
                dimensional_tolerance_meters: 0.00005,
                surface_roughness_ra_meters: 1.6e-6,
                angular_tolerance_radians: 0.05_f64.to_radians(),
                min_curvature_radius_meters: 0.003,
                manufacturing_cost_base_cny: 1000.0,
                feasibility_threshold: 0.5,
                jerk_limit_m_per_s3: 1000.0,
                weights: ToleranceWeights {
                    curvature: 0.35,
                    lift_tolerance: 0.25,
                    pressure_angle: 0.2,
                    jerk: 0.1,
                    surface: 0.1,
                },
            },
            search_grid: SearchGridConfig {
                radius_steps: 10,
                lift_steps: 10,
                profile_types: vec![
                    "cycloidal".into(),
                    "harmonic".into(),
                    "trapezoidal".into(),
                    "polynomial".into(),
                ],
            },
            scoring_weights: ScoringWeightsConfig {
                efficiency_pure: 0.3,
                efficiency_with_constraints: 0.7,
            },
            pressure_angle_penalty: PressureAnglePenaltyConfig {
                threshold_degrees: 30.0,
                decay_exponent: 2.0,
            },
        }
    }
}
