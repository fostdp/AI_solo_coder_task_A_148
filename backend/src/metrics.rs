use prometheus::{IntCounter, IntGauge, Histogram, HistogramOpts, Opts, Registry};
use lazy_static::lazy_static;
use std::sync::Arc;

lazy_static! {
    pub static ref REGISTRY: Arc<Registry> = Arc::new(Registry::new());

    pub static ref MQTT_MESSAGES_RECEIVED: IntCounter =
        IntCounter::with_opts(Opts::new("mqtt_messages_received_total", "Total MQTT messages received"))
            .unwrap();

    pub static ref MQTT_MESSAGES_INVALID: IntCounter =
        IntCounter::with_opts(Opts::new("mqtt_messages_invalid_total", "Total invalid MQTT messages"))
            .unwrap();

    pub static ref SIMULATIONS_RUN: IntCounter =
        IntCounter::with_opts(Opts::new("simulations_run_total", "Total dynamics simulations run"))
            .unwrap();

    pub static ref OPTIMIZATIONS_RUN: IntCounter =
        IntCounter::with_opts(Opts::new("optimizations_run_total", "Total optimization runs"))
            .unwrap();

    pub static ref ALERTS_GENERATED: IntCounter =
        IntCounter::with_opts(Opts::new("alerts_generated_total", "Total alerts generated"))
            .unwrap();

    pub static ref ALERTS_SUPPRESSED: IntCounter =
        IntCounter::with_opts(Opts::new("alerts_suppressed_total", "Total alerts suppressed by dedup"))
            .unwrap();

    pub static ref ACTIVE_DEVICES: IntGauge =
        IntGauge::with_opts(Opts::new("active_devices", "Number of active devices"))
            .unwrap();

    pub static ref WEBSOCKET_CONNECTIONS: IntGauge =
        IntGauge::with_opts(Opts::new("websocket_connections", "Current WebSocket connections"))
            .unwrap();

    pub static ref SIMULATION_DURATION: Histogram =
        Histogram::with_opts(HistogramOpts::new("simulation_duration_seconds", "Simulation execution duration")
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]))
            .unwrap();

    pub static ref OPTIMIZATION_DURATION: Histogram =
        Histogram::with_opts(HistogramOpts::new("optimization_duration_seconds", "Optimization execution duration")
            .buckets(vec![0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]))
            .unwrap();

    pub static ref POUNDING_FORCE: Histogram =
        Histogram::with_opts(HistogramOpts::new("pounding_force_newtons", "Pounding force distribution")
            .buckets(vec![50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0]))
            .unwrap();
}

pub fn init_metrics() {
    REGISTRY.register(Box::new(MQTT_MESSAGES_RECEIVED.clone())).unwrap();
    REGISTRY.register(Box::new(MQTT_MESSAGES_INVALID.clone())).unwrap();
    REGISTRY.register(Box::new(SIMULATIONS_RUN.clone())).unwrap();
    REGISTRY.register(Box::new(OPTIMIZATIONS_RUN.clone())).unwrap();
    REGISTRY.register(Box::new(ALERTS_GENERATED.clone())).unwrap();
    REGISTRY.register(Box::new(ALERTS_SUPPRESSED.clone())).unwrap();
    REGISTRY.register(Box::new(ACTIVE_DEVICES.clone())).unwrap();
    REGISTRY.register(Box::new(WEBSOCKET_CONNECTIONS.clone())).unwrap();
    REGISTRY.register(Box::new(SIMULATION_DURATION.clone())).unwrap();
    REGISTRY.register(Box::new(OPTIMIZATION_DURATION.clone())).unwrap();
    REGISTRY.register(Box::new(POUNDING_FORCE.clone())).unwrap();
}

pub fn gather_metrics() -> String {
    prometheus::TextEncoder::new().encode_to_string(&REGISTRY.gather()).unwrap()
}
