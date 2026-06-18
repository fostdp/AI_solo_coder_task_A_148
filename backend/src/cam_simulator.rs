use crate::message_bus::{SimulatorCmdRx, SimulatorCommand};
use crate::dynamics::CamDynamicsSimulator;
use crate::models::{DeviceInfo, DynamicsResult, SensorData, CamPoint};
use crate::config::DynamicsConfig;

use std::collections::HashMap;
use tracing::{info, error};

pub struct CamSimulatorService {
    simulators: HashMap<String, CamDynamicsSimulator>,
    config: DynamicsConfig,
    cmd_rx: SimulatorCmdRx,
}

impl CamSimulatorService {
    pub fn new(cmd_rx: SimulatorCmdRx, config: DynamicsConfig) -> Self {
        CamSimulatorService {
            simulators: HashMap::new(),
            config,
            cmd_rx,
        }
    }

    pub fn with_devices(mut self, devices: &[DeviceInfo]) -> Self {
        for d in devices {
            let sim = CamDynamicsSimulator::new_with_config(d.clone(), &self.config);
            self.simulators.insert(d.device_id.clone(), sim);
        }
        self
    }

    pub async fn run(mut self) {
        info!("CamSimulatorService started");
        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                SimulatorCommand::Simulate { sensor, device, reply } => {
                    let start = std::time::Instant::now();
                    let result = self.handle_simulate(sensor, device);
                    crate::metrics::SIMULATION_DURATION.observe(start.elapsed().as_secs_f64());
                    crate::metrics::SIMULATIONS_RUN.inc();
                    crate::metrics::POUNDING_FORCE.observe(result.pounding_force);
                    let _ = reply.send(result);
                }
                SimulatorCommand::GenerateProfile { device, base_radius, lift, num_points, reply } => {
                    let result = self.handle_generate_profile(device, base_radius, lift, num_points);
                    let _ = reply.send(result);
                }
                SimulatorCommand::UpdateDevices(list) => {
                    for (id, device) in list {
                        let sim = CamDynamicsSimulator::new_with_config(device, &self.config);
                        self.simulators.insert(id, sim);
                    }
                    info!("Updated devices in CamSimulator");
                }
            }
        }
        error!("CamSimulatorService stopped (cmd channel closed)");
    }

    fn handle_simulate(&mut self, sensor: SensorData, device: DeviceInfo) -> DynamicsResult {
        let device_id = sensor.device_id.clone();
        if !self.simulators.contains_key(&device_id) {
            let sim = CamDynamicsSimulator::new_with_config(device, &self.config);
            self.simulators.insert(device_id.clone(), sim);
        }
        let sim = self.simulators.get_mut(&device_id).unwrap();
        sim.simulate(&sensor)
    }

    fn handle_generate_profile(
        &mut self,
        device: DeviceInfo,
        base_radius: f64,
        lift: f64,
        num_points: usize,
    ) -> Vec<CamPoint> {
        let device_id = device.device_id.clone();
        let sim = self.simulators
            .entry(device_id.clone())
            .or_insert_with(|| CamDynamicsSimulator::new_with_config(device, &self.config));
        sim.generate_cam_profile(base_radius, lift, num_points)
    }
}
