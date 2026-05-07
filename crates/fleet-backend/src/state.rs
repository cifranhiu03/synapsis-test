//! In-memory fleet state.
//!
//! Two reasons it's split out from the handlers:
//!   1. The data model is what's worth reviewing — handlers are mostly
//!      glue. Keeping it here makes the shape obvious.
//!   2. Tests can drive `AppState` directly without spinning up axum.
//!
//! Storage choice: `DashMap` for current-fleet snapshot (tiny, hot-read,
//! sharded under contention), and a `tokio::broadcast` channel as the
//! fanout bus (lossy by design for slow consumers — see SSE handler).
//! Per-truck history rings join in D4.

use dashmap::DashMap;
use fleet_proto::v1::{FleetUpdate, Telemetry};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;

/// Capacity of the broadcast bus. Sized for ~5s of fanout at 5 trucks ×
/// 2 Hz (≈50 frames) plus headroom for health events; slow clients beyond
/// this lag and receive a `resync` (handled by the SSE layer in D4).
pub const BROADCAST_CAPACITY: usize = 256;

#[derive(Clone, Debug)]
pub struct TruckSnapshot {
    pub telemetry: Telemetry,
    pub received_at: Instant,
}

#[derive(Clone)]
pub struct AppState {
    /// Latest known telemetry per truck. Reads dominate here — DashMap's
    /// per-shard locking keeps the snapshot endpoint cheap even while
    /// ingest is writing.
    pub fleet: Arc<DashMap<String, TruckSnapshot>>,

    /// Fanout bus for live updates. `Sender::send` returning Err means no
    /// active receivers — that's fine and not an error.
    pub tx: broadcast::Sender<FleetUpdate>,
}

impl AppState {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            fleet: Arc::new(DashMap::new()),
            tx,
        }
    }

    /// Apply an incoming telemetry frame: replace the per-truck snapshot
    /// and publish a `FleetUpdate::Telemetry` to the bus. Returns the
    /// snapshot that was stored so callers (e.g., the health classifier
    /// in D4) can act on the post-state without re-reading the map.
    pub fn apply_telemetry(&self, telemetry: Telemetry) -> TruckSnapshot {
        let snap = TruckSnapshot {
            telemetry: telemetry.clone(),
            received_at: Instant::now(),
        };
        self.fleet.insert(telemetry.truck_id.clone(), snap.clone());

        let update = FleetUpdate {
            payload: Some(fleet_proto::v1::fleet_update::Payload::Telemetry(telemetry)),
        };
        // Ignore "no receivers" — the bus is best-effort.
        let _ = self.tx.send(update);
        snap
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
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
    fn apply_telemetry_replaces_existing_snapshot() {
        let state = AppState::new();
        state.apply_telemetry(frame("T-01", 0.9));
        state.apply_telemetry(frame("T-01", 0.5));
        let s = state.fleet.get("T-01").unwrap();
        assert_eq!(s.telemetry.fuel_pct, 0.5);
        assert_eq!(state.fleet.len(), 1);
    }

    #[test]
    fn broadcast_with_no_receivers_is_not_an_error() {
        let state = AppState::new();
        // Should not panic and should not return Err to callers.
        state.apply_telemetry(frame("T-01", 0.9));
    }

    #[tokio::test]
    async fn subscribers_see_published_updates() {
        let state = AppState::new();
        let mut rx = state.tx.subscribe();
        state.apply_telemetry(frame("T-02", 0.8));
        let got = rx.recv().await.expect("recv");
        match got.payload {
            Some(fleet_proto::v1::fleet_update::Payload::Telemetry(t)) => {
                assert_eq!(t.truck_id, "T-02");
            }
            other => panic!("unexpected payload: {other:?}"),
        }
    }
}
