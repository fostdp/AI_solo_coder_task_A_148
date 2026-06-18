use crate::message_bus::{OptimizerCmdRx, OptimizerCommand};
use crate::config::OptimizationConfig;
use crate::models::{DeviceInfo, OptimizationResult, OptimizationRequest};
use crate::optimization::{PoundingOptimizer, ToleranceAnalysis};
use crate::config::DynamicsConfig;

use tracing::{info, error};

pub struct ForceOptimizerService {
    config: OptimizationConfig,
    dynamics_config: DynamicsConfig,
    cmd_rx: OptimizerCmdRx,
}

impl ForceOptimizerService {
    pub fn new(
        cmd_rx: OptimizerCmdRx,
        config: OptimizationConfig,
        dynamics_config: DynamicsConfig,
    ) -> Self {
        ForceOptimizerService {
            config,
            dynamics_config,
            cmd_rx,
        }
    }

    pub async fn run(mut self) {
        info!("ForceOptimizerService started");
        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                OptimizerCommand::Optimize { request, device, reply } => {
                    let start = std::time::Instant::now();
                    let result = self.handle_optimize(request, device);
                    crate::metrics::OPTIMIZATION_DURATION.observe(start.elapsed().as_secs_f64());
                    crate::metrics::OPTIMIZATIONS_RUN.inc();
                    let _ = reply.send(result);
                }
            }
        }
        error!("ForceOptimizerService stopped (cmd channel closed)");
    }

    fn handle_optimize(&self, request: OptimizationRequest, device: DeviceInfo) -> OptimizationResult {
        let tolerance = ToleranceAnalysis::from_config(&self.config.tolerance);
        let optimizer = PoundingOptimizer::with_config(
            device,
            tolerance,
            self.dynamics_config.clone(),
        );
        optimizer.optimize(&request)
    }
}
