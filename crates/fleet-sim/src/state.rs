//! Haul-truck lifecycle state machine.
//!
//! A truck cycles deterministically:
//!   Idle → LoadingQueue → Loading → Hauling → AtCrusher → Dumping → Returning → Idle
//! and occasionally lingers in Idle. Each state has a (min, max) dwell time
//! sampled per visit so five trucks running the same machine still look like
//! five different trucks.

use fleet_proto::v1::TruckState as ProtoState;
use rand::Rng;
use std::time::Duration;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SimState {
    Idle,
    LoadingQueue,
    Loading,
    Hauling,
    AtCrusher,
    Dumping,
    Returning,
}

impl SimState {
    pub fn to_proto(self) -> ProtoState {
        match self {
            SimState::Idle => ProtoState::Idle,
            SimState::LoadingQueue => ProtoState::LoadingQueue,
            SimState::Loading => ProtoState::Loading,
            SimState::Hauling => ProtoState::Hauling,
            SimState::AtCrusher => ProtoState::AtCrusher,
            SimState::Dumping => ProtoState::Dumping,
            SimState::Returning => ProtoState::Returning,
        }
    }

    /// Returns the next state and the dwell time the truck will spend in it.
    /// Dwell ranges are tuned so a full cycle is roughly 4–7 minutes — short
    /// enough that a reviewer sees state changes within a minute of bring-up.
    pub fn advance<R: Rng + ?Sized>(self, rng: &mut R) -> (Self, Duration) {
        let (next, range_s) = match self {
            // Most cycles flow Idle → LoadingQueue, but ~20% of the time the
            // truck lingers in Idle so the dashboard shows a non-uniform fleet.
            SimState::Idle => {
                if rng.gen_bool(0.2) {
                    (SimState::Idle, (20, 60))
                } else {
                    (SimState::LoadingQueue, (10, 30))
                }
            }
            SimState::LoadingQueue => (SimState::Loading, (15, 30)),
            SimState::Loading => (SimState::Hauling, (60, 120)),
            SimState::Hauling => (SimState::AtCrusher, (10, 25)),
            SimState::AtCrusher => (SimState::Dumping, (15, 30)),
            SimState::Dumping => (SimState::Returning, (50, 100)),
            SimState::Returning => (SimState::Idle, (5, 15)),
        };
        let secs = rng.gen_range(range_s.0..=range_s.1);
        (next, Duration::from_secs(secs))
    }

    /// Whether this state implies the truck is moving along its route.
    pub fn is_moving(self) -> bool {
        matches!(self, SimState::Hauling | SimState::Returning)
    }

    /// Whether the truck carries a load while in this state.
    pub fn is_loaded(self) -> bool {
        matches!(self, SimState::Hauling | SimState::AtCrusher | SimState::Dumping)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn full_cycle_visits_all_states_within_30_min() {
        let mut rng = StdRng::seed_from_u64(7);
        let mut state = SimState::Idle;
        let mut elapsed = Duration::ZERO;
        let mut seen = std::collections::HashSet::new();
        seen.insert(state);

        // Simulate up to a wall-clock budget.
        for _ in 0..200 {
            let (next, dwell) = state.advance(&mut rng);
            elapsed += dwell;
            state = next;
            seen.insert(state);
            if seen.len() == 7 {
                break;
            }
        }
        assert_eq!(seen.len(), 7, "did not visit every state");
        assert!(
            elapsed < Duration::from_secs(30 * 60),
            "cycle too slow: {elapsed:?}"
        );
    }
}
