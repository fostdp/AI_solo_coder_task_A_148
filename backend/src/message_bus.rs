use crate::models::{
    SensorData, DynamicsResult, Alert, DeviceInfo,
    OptimizationRequest, OptimizationResult, CamPoint,
};
use tokio::sync::oneshot;

#[derive(Debug, Clone)]
pub enum BusMessage {
    SensorReceived(SensorData),
    DynamicsComputed(DynamicsResult),
    AlertTriggered(Alert),
    DevicesLoaded(Vec<DeviceInfo>),
    Shutdown,
}

#[derive(Debug)]
pub enum SimulatorCommand {
    Simulate {
        sensor: SensorData,
        device: DeviceInfo,
        reply: oneshot::Sender<DynamicsResult>,
    },
    GenerateProfile {
        device: DeviceInfo,
        base_radius: f64,
        lift: f64,
        num_points: usize,
        reply: oneshot::Sender<Vec<CamPoint>>,
    },
    UpdateDevices(Vec<(String, DeviceInfo)>),
}

#[derive(Debug)]
pub enum OptimizerCommand {
    Optimize {
        request: OptimizationRequest,
        device: DeviceInfo,
        reply: oneshot::Sender<OptimizationResult>,
    },
}

#[derive(Debug)]
pub enum AlarmCommand {
    CheckAlerts {
        sensor: SensorData,
        device: DeviceInfo,
        reply: oneshot::Sender<Vec<Alert>>,
    },
}

pub type SensorTx = tokio::sync::mpsc::UnboundedSender<SensorData>;
pub type SensorRx = tokio::sync::mpsc::UnboundedReceiver<SensorData>;

pub type DynamicsTx = tokio::sync::mpsc::UnboundedSender<DynamicsResult>;
pub type DynamicsRx = tokio::sync::mpsc::UnboundedReceiver<DynamicsResult>;

pub type AlertTx = tokio::sync::broadcast::Sender<Alert>;
pub type AlertRx = tokio::sync::broadcast::Receiver<Alert>;

pub type SimulatorCmdTx = tokio::sync::mpsc::UnboundedSender<SimulatorCommand>;
pub type SimulatorCmdRx = tokio::sync::mpsc::UnboundedReceiver<SimulatorCommand>;

pub type OptimizerCmdTx = tokio::sync::mpsc::UnboundedSender<OptimizerCommand>;
pub type OptimizerCmdRx = tokio::sync::mpsc::UnboundedReceiver<OptimizerCommand>;

pub type AlarmCmdTx = tokio::sync::mpsc::UnboundedSender<AlarmCommand>;
pub type AlarmCmdRx = tokio::sync::mpsc::UnboundedReceiver<AlarmCommand>;
