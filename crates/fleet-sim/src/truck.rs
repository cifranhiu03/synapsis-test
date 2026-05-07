//! Per-truck simulation: ties together state machine, route, and the
//! synthetic faults a dispatcher should see on the dashboard.

use crate::route::{noisy_fix, Route};
use crate::state::SimState;
use fleet_proto::v1::{LoadStatus, Telemetry, TruckState as ProtoState};
use rand::Rng;
use std::time::{Duration, Instant};

/// Faults a truck can emit. Wired per-truck so the demo always shows
/// at least one over-rev and one fuel anomaly without needing the
/// reviewer to wait an hour.
#[derive(Copy, Clone, Debug, Default)]
pub struct FaultPlan {
    pub over_rev_every: Option<Duration>,
    pub over_rev_for: Duration,
    pub fuel_drop_at: Option<Duration>,
    pub fuel_drop_pct: f32,
    pub gps_dropout_chance: f64,
}

pub struct Truck {
    pub id: String,
    route: Route,
    state: SimState,
    state_until: Instant,
    /// Progress along the route in [0, 1]. Increases during Hauling,
    /// decreases during Returning.
    progress: f64,
    fuel_pct: f32,
    fault: FaultPlan,
    started_at: Instant,
    last_over_rev: Option<Instant>,
    fuel_drop_done: bool,
}

impl Truck {
    pub fn new<R: Rng + ?Sized>(
        id: impl Into<String>,
        route: Route,
        fault: FaultPlan,
        rng: &mut R,
    ) -> Self {
        let now = Instant::now();
        // Stagger initial states so the fleet doesn't move in lockstep.
        let initial_states = [
            SimState::Idle,
            SimState::LoadingQueue,
            SimState::Hauling,
            SimState::AtCrusher,
            SimState::Returning,
        ];
        let state = initial_states[rng.gen_range(0..initial_states.len())];
        let (_, dwell) = state.advance(rng);
        let progress = match state {
            SimState::Idle | SimState::LoadingQueue | SimState::Loading => 0.0,
            SimState::Hauling => rng.gen_range(0.0..0.7),
            SimState::AtCrusher | SimState::Dumping => 1.0,
            SimState::Returning => rng.gen_range(0.3..1.0),
        };
        Self {
            id: id.into(),
            route,
            state,
            state_until: now + dwell,
            progress,
            fuel_pct: rng.gen_range(0.55..=0.95),
            fault,
            started_at: now,
            last_over_rev: None,
            fuel_drop_done: false,
        }
    }

    /// Advance state if dwell elapsed, then progress along the route by dt.
    pub fn step<R: Rng + ?Sized>(&mut self, now: Instant, dt: Duration, rng: &mut R) {
        if now >= self.state_until {
            let (next, dwell) = self.state.advance(rng);
            self.state = next;
            self.state_until = now + dwell;
            // Snap progress at state boundaries.
            match next {
                SimState::Loading | SimState::Idle | SimState::LoadingQueue => self.progress = 0.0,
                SimState::AtCrusher | SimState::Dumping => self.progress = 1.0,
                _ => {}
            }
        }

        if self.state.is_moving() {
            // Cruise speed ~25–35 km/h loaded, ~40–50 km/h empty.
            let cruise_kmh = if self.state.is_loaded() { 30.0 } else { 45.0 };
            let metres_per_s = cruise_kmh * 1000.0 / 3600.0;
            let total = self.route.total_m().max(1.0);
            let delta = metres_per_s * dt.as_secs_f64() / total;
            self.progress = match self.state {
                SimState::Hauling => (self.progress + delta).min(1.0),
                SimState::Returning => (self.progress - delta).max(0.0),
                _ => self.progress,
            };
        }

        // Fuel drains faster while moving and especially while loaded.
        let drain_pct_per_s = match self.state {
            SimState::Idle | SimState::LoadingQueue => 0.0005,
            SimState::Loading | SimState::AtCrusher | SimState::Dumping => 0.0010,
            SimState::Hauling => 0.0035,
            SimState::Returning => 0.0025,
        };
        self.fuel_pct = (self.fuel_pct - drain_pct_per_s * dt.as_secs_f64() as f32).max(0.0);

        // Scheduled fuel anomaly: a single sharp drop, fires once.
        if let Some(at) = self.fault.fuel_drop_at {
            if !self.fuel_drop_done && now.duration_since(self.started_at) >= at {
                self.fuel_pct = (self.fuel_pct - self.fault.fuel_drop_pct).max(0.0);
                self.fuel_drop_done = true;
            }
        }
    }

    /// Render a `Telemetry` frame for the current instant.
    pub fn telemetry<R: Rng + ?Sized>(&mut self, now: Instant, ts_unix_ms: i64, rng: &mut R) -> Telemetry {
        let pos = self.route.position_at(self.progress);

        // GPS dropout: occasionally omit the fix and flag a stale hdop.
        let gps = if rng.gen_bool(self.fault.gps_dropout_chance) {
            None
        } else {
            Some(noisy_fix(pos, rng))
        };

        let (speed_kmh, base_rpm) = if self.state.is_moving() {
            let cruise = if self.state.is_loaded() { 30.0 } else { 45.0 };
            let s = cruise + rng.gen_range(-3.0..=3.0);
            let r = (1200.0 + s * 18.0) as u32;
            (s as f64, r)
        } else {
            (0.0_f64, rng.gen_range(650..=850))
        };

        // Sustained over-rev burst: clamp last_over_rev once we cross the
        // periodic threshold and emit elevated RPM until the burst window
        // expires. Health classifier needs ≥5s of >2200 RPM to fire, so
        // an 8s burst gives margin around timing skew.
        let mut rpm = base_rpm;
        if let Some(period) = self.fault.over_rev_every {
            let since_start = now.duration_since(self.started_at);
            let cycle = since_start.as_secs() / period.as_secs();
            if cycle > 0 {
                let cycle_start = self.started_at + period * cycle as u32;
                if now.duration_since(cycle_start) <= self.fault.over_rev_for {
                    self.last_over_rev = Some(now);
                    rpm = rng.gen_range(2300..=2500);
                }
            }
        }

        let load = if self.state.is_loaded() {
            LoadStatus::Loaded
        } else {
            LoadStatus::Empty
        };

        Telemetry {
            truck_id: self.id.clone(),
            ts_unix_ms,
            gps,
            speed_kmh,
            rpm,
            load: load as i32,
            fuel_pct: self.fuel_pct,
            state: self.state.to_proto() as i32,
        }
    }

    /// Test/debug accessor.
    pub fn proto_state(&self) -> ProtoState {
        self.state.to_proto()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::fleet_routes;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn telemetry_load_matches_state() {
        let mut rng = StdRng::seed_from_u64(1);
        let route = fleet_routes().remove(0);
        let mut t = Truck::new("T-1", route, FaultPlan::default(), &mut rng);
        // Force a known state.
        t.state = SimState::Hauling;
        let frame = t.telemetry(Instant::now(), 0, &mut rng);
        assert_eq!(frame.load, LoadStatus::Loaded as i32);
        t.state = SimState::Returning;
        let frame = t.telemetry(Instant::now(), 0, &mut rng);
        assert_eq!(frame.load, LoadStatus::Empty as i32);
    }

    #[test]
    fn fuel_monotonically_decreases_without_anomaly() {
        let mut rng = StdRng::seed_from_u64(2);
        let route = fleet_routes().remove(0);
        let mut t = Truck::new("T-1", route, FaultPlan::default(), &mut rng);
        let start = t.fuel_pct;
        for _ in 0..100 {
            t.step(Instant::now(), Duration::from_secs(1), &mut rng);
        }
        assert!(t.fuel_pct < start, "fuel did not decrease");
    }
}
