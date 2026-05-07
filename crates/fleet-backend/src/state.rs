//! In-memory fleet state.
//!
//! Storage choices:
//!   * `DashMap<truck_id, TruckSnapshot>` for current state — sharded
//!     locking keeps the snapshot endpoint cheap under concurrent ingest.
//!   * `DashMap<truck_id, Mutex<VecDeque<Telemetry>>>` for per-truck
//!     history — bounded ring buffer (~10 min at 2 Hz). The mutex is
//!     fine: only ingest writes, only the history endpoint reads, and
//!     both are short-held.
//!   * `tokio::broadcast` channel for live fanout — lossy by design for
//!     slow consumers; the SSE handler turns `Lagged` into a `resync`.
//!   * `DashMap<truck_id, HashSet<HealthKind>>` for currently-active
//!     alerts — used to dedupe so the bus only carries
//!     inactive→active transitions, not every classify() pass.

use crate::health;
use dashmap::DashMap;
use fleet_proto::v1::{fleet_update::Payload, FleetUpdate, HealthEvent, HealthKind, Severity, Telemetry};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;

pub const BROADCAST_CAPACITY: usize = 256;
pub const HISTORY_CAPACITY: usize = 1200; // 10 min @ 2 Hz

#[derive(Clone, Debug)]
pub struct TruckSnapshot {
    pub telemetry: Telemetry,
    pub received_at: Instant,
}

#[derive(Clone)]
pub struct AppState {
    pub fleet: Arc<DashMap<String, TruckSnapshot>>,
    pub history: Arc<DashMap<String, Mutex<std::collections::VecDeque<Telemetry>>>>,
    pub alerts: Arc<DashMap<String, HashSet<HealthKind>>>,
    pub tx: broadcast::Sender<FleetUpdate>,
}

impl AppState {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            fleet: Arc::new(DashMap::new()),
            history: Arc::new(DashMap::new()),
            alerts: Arc::new(DashMap::new()),
            tx,
        }
    }

    /// Apply a telemetry frame, push history, run the classifier, and
    /// publish a `Telemetry` update plus any newly-active health events.
    pub fn apply_telemetry(&self, telemetry: Telemetry) {
        let truck_id = telemetry.truck_id.clone();

        // Snapshot.
        self.fleet.insert(
            truck_id.clone(),
            TruckSnapshot {
                telemetry: telemetry.clone(),
                received_at: Instant::now(),
            },
        );

        // History — bounded ring buffer.
        let entry = self
            .history
            .entry(truck_id.clone())
            .or_insert_with(|| Mutex::new(std::collections::VecDeque::with_capacity(HISTORY_CAPACITY)));
        {
            let mut buf = entry.lock().expect("history mutex poisoned");
            if buf.len() == HISTORY_CAPACITY {
                buf.pop_front();
            }
            buf.push_back(telemetry.clone());
        }

        // Live fanout (best-effort: send returns Err only when there are
        // no receivers, which is fine).
        let ts = telemetry.ts_unix_ms;
        let _ = self.tx.send(FleetUpdate {
            payload: Some(Payload::Telemetry(telemetry)),
        });

        // Health classification + alert dedup.
        let active = {
            let buf = entry.lock().expect("history mutex poisoned");
            let samples: Vec<Telemetry> = buf.iter().cloned().collect();
            health::classify(&samples, ts)
        };
        let prev: HashSet<HealthKind> = self
            .alerts
            .get(&truck_id)
            .map(|r| r.clone())
            .unwrap_or_default();
        let new_alerts: Vec<HealthKind> = active.difference(&prev).cloned().collect();
        self.alerts.insert(truck_id.clone(), active);

        for kind in new_alerts {
            let event = HealthEvent {
                truck_id: truck_id.clone(),
                ts_unix_ms: ts,
                kind: kind as i32,
                severity: severity_for(kind) as i32,
                message: message_for(kind).into(),
            };
            let _ = self.tx.send(FleetUpdate {
                payload: Some(Payload::Health(event)),
            });
        }
    }

    /// Read a copy of the per-truck history filtered by `since_ms`.
    pub fn history_since(&self, truck_id: &str, since_ms: i64) -> Vec<Telemetry> {
        let Some(entry) = self.history.get(truck_id) else {
            return Vec::new();
        };
        let buf = entry.lock().expect("history mutex poisoned");
        buf.iter()
            .filter(|t| t.ts_unix_ms >= since_ms)
            .cloned()
            .collect()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

fn severity_for(kind: HealthKind) -> Severity {
    match kind {
        HealthKind::OverRev | HealthKind::UnsafeSpeedUnderLoad | HealthKind::FuelAnomaly => Severity::Fault,
        HealthKind::ExcessiveIdle | HealthKind::LoadMismatch | HealthKind::Stuck => Severity::Warn,
        HealthKind::GpsStale => Severity::Info,
        HealthKind::Unspecified => Severity::Unspecified,
    }
}

fn message_for(kind: HealthKind) -> &'static str {
    match kind {
        HealthKind::OverRev => "Sustained engine over-rev",
        HealthKind::UnsafeSpeedUnderLoad => "Unsafe speed while loaded",
        HealthKind::ExcessiveIdle => "Excessive idle time",
        HealthKind::FuelAnomaly => "Fuel level dropped sharply",
        HealthKind::GpsStale => "GPS fix is stale",
        HealthKind::Stuck => "No telemetry — truck may be stuck",
        HealthKind::LoadMismatch => "Load status disagrees with state",
        HealthKind::Unspecified => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleet_proto::v1::{LoadStatus, TruckState};

    fn frame(id: &str, fuel: f32) -> Telemetry {
        Telemetry {
            truck_id: id.into(),
            ts_unix_ms: 0,
            gps: None,
            speed_kmh: 0.0,
            rpm: 0,
            load: LoadStatus::Empty as i32,
            fuel_pct: fuel,
            state: TruckState::Idle as i32,
        }
    }

    #[test]
    fn apply_telemetry_replaces_snapshot_and_pushes_history() {
        let state = AppState::new();
        for i in 0..3 {
            let mut f = frame("T-01", 0.9 - i as f32 * 0.01);
            f.ts_unix_ms = i;
            state.apply_telemetry(f);
        }
        assert_eq!(state.fleet.len(), 1);
        let hist = state.history_since("T-01", 0);
        assert_eq!(hist.len(), 3);
    }

    #[test]
    fn history_caps_at_capacity_and_evicts_oldest() {
        let state = AppState::new();
        for i in 0..(HISTORY_CAPACITY as i64 + 5) {
            let mut f = frame("T-01", 0.9);
            f.ts_unix_ms = i;
            state.apply_telemetry(f);
        }
        let hist = state.history_since("T-01", 0);
        assert_eq!(hist.len(), HISTORY_CAPACITY);
        // Oldest remaining sample's ts is the (5)th frame we pushed.
        assert_eq!(hist.first().unwrap().ts_unix_ms, 5);
    }

    #[tokio::test]
    async fn slow_subscriber_eventually_lags() {
        // Subscriber that never reads should hit Lagged once we exceed
        // BROADCAST_CAPACITY. This is the contract the SSE layer relies on.
        let state = AppState::new();
        let mut rx = state.tx.subscribe();
        for i in 0..(BROADCAST_CAPACITY + 50) {
            let mut f = frame("T-01", 0.9);
            f.ts_unix_ms = i as i64;
            state.apply_telemetry(f);
        }
        // Drain: first recv should be Lagged because we never read.
        let err = rx.try_recv();
        assert!(matches!(err, Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_))));
    }
}
