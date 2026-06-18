use crate::message_bus::{AlarmCmdRx, AlarmCommand};
use crate::alerts::AlertDetector;
use crate::models::{Alert, DeviceInfo, SensorData};

use tokio::sync::broadcast;
use log::{info, error, warn};
use std::collections::{HashMap, VecDeque};
use std::time::{Instant, Duration};

const ALERT_DEDUP_WINDOW_SECS: u64 = 5;
const MAX_ALERTS_PER_WINDOW: usize = 3;

pub struct AlarmWsService {
    cmd_rx: AlarmCmdRx,
    alert_tx: broadcast::Sender<Alert>,
    detectors: HashMap<String, (AlertDetector, RateLimiter)>,
}

struct RateLimiter {
    recent_alerts: VecDeque<(String, Instant)>,
}

impl RateLimiter {
    fn new() -> Self {
        RateLimiter {
            recent_alerts: VecDeque::new(),
        }
    }

    fn should_send(&mut self, alert_type: &str, level: &str) -> bool {
        let now = Instant::now();
        let key = format!("{}:{}", alert_type, level);

        while let Some((_, instant)) = self.recent_alerts.front() {
            if now.duration_since(*instant) > Duration::from_secs(ALERT_DEDUP_WINDOW_SECS) {
                self.recent_alerts.pop_front();
            } else {
                break;
            }
        }

        let count = self.recent_alerts.iter()
            .filter(|(k, _)| k == &key)
            .count();

        if count >= MAX_ALERTS_PER_WINDOW {
            return false;
        }

        self.recent_alerts.push_back((key, now));
        true
    }
}

impl AlarmWsService {
    pub fn new(cmd_rx: AlarmCmdRx, alert_tx: broadcast::Sender<Alert>) -> Self {
        AlarmWsService {
            cmd_rx,
            alert_tx,
            detectors: HashMap::new(),
        }
    }

    pub async fn run(mut self) {
        info!("AlarmWsService started");
        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                AlarmCommand::CheckAlerts { sensor, device, reply } => {
                    let device_clone = device.clone();
                    let alerts = self.handle_check(sensor, device);
                    for alert in &alerts {
                        let key = format!("{}:{}", alert.alert_type, alert.alert_level);
                        let rate_limiter = &mut self.detectors
                            .entry(alert.device_id.clone())
                            .or_insert_with(|| (AlertDetector::new(&device_clone), RateLimiter::new()))
                            .1;
                        if rate_limiter.should_send(&alert.alert_type, &alert.alert_level) {
                            if self.alert_tx.send(alert.clone()).is_err() {
                                warn!("Alert broadcast channel has no subscribers");
                            }
                        }
                    }
                    let _ = reply.send(alerts);
                }
            }
        }
        error!("AlarmWsService stopped (cmd channel closed)");
    }

    fn handle_check(&mut self, sensor: SensorData, device: DeviceInfo) -> Vec<Alert> {
        let device_id = sensor.device_id.clone();
        let entry = self.detectors
            .entry(device_id)
            .or_insert_with(|| (AlertDetector::new(&device), RateLimiter::new()));
        entry.0.detect(&sensor)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Alert> {
        self.alert_tx.subscribe()
    }
}
