//! Rule-based health classifier.
//!
//! `classify` is a pure function over a time-ordered window of telemetry
//! samples. It returns the **currently active** set of health conditions
//! for a truck — the caller decides whether a particular condition has
//! transitioned from inactive→active and should fan out a `HealthEvent`.
//! That separation keeps the rules trivially testable and avoids state
//! leaking into what is otherwise data analysis.
//!
//! Thresholds are picked to be generous to noise: a single-sample RPM
//! spike does not trigger over-rev, and a one-second speed blip while
//! loaded does not trigger unsafe-speed. Each rule is documented with
//! its rationale in the function body so a reviewer can argue against
//! a specific number rather than the whole approach.

use fleet_proto::v1::{HealthKind, LoadStatus, Telemetry, TruckState};
use std::collections::HashSet;

/// Tunables. Centralised so README and tests can reference one place.
pub const OVER_REV_RPM: u32 = 2200;
pub const OVER_REV_MIN_DURATION_MS: i64 = 5_000;
pub const UNSAFE_SPEED_KMH: f64 = 40.0;
pub const UNSAFE_SPEED_MIN_DURATION_MS: i64 = 3_000;
pub const EXCESSIVE_IDLE_MS: i64 = 10 * 60 * 1_000;
pub const FUEL_WINDOW_MS: i64 = 60 * 1_000;
pub const FUEL_DROP_PCT: f32 = 0.05;
pub const GPS_STALE_MS: i64 = 10_000;
pub const STUCK_NO_TELEMETRY_MS: i64 = 2 * 60 * 1_000;

/// Run all rules over `samples` (must be sorted ascending by `ts_unix_ms`).
/// `now_ms` is the current wall-clock; using it explicitly keeps the
/// function pure and tests deterministic.
pub fn classify(samples: &[Telemetry], now_ms: i64) -> HashSet<HealthKind> {
    let mut out = HashSet::new();
    if samples.is_empty() {
        return out;
    }

    if sustained(samples, OVER_REV_MIN_DURATION_MS, |t| t.rpm > OVER_REV_RPM) {
        out.insert(HealthKind::OverRev);
    }

    if sustained(samples, UNSAFE_SPEED_MIN_DURATION_MS, |t| {
        t.speed_kmh > UNSAFE_SPEED_KMH && t.load == LoadStatus::Loaded as i32
    }) {
        out.insert(HealthKind::UnsafeSpeedUnderLoad);
    }

    if excessive_idle(samples, now_ms) {
        out.insert(HealthKind::ExcessiveIdle);
    }

    if fuel_anomaly(samples) {
        out.insert(HealthKind::FuelAnomaly);
    }

    if gps_stale(samples, now_ms) {
        out.insert(HealthKind::GpsStale);
    }

    if stuck(samples, now_ms) {
        out.insert(HealthKind::Stuck);
    }

    if load_mismatch(samples) {
        out.insert(HealthKind::LoadMismatch);
    }

    out
}

/// True iff the predicate is continuously satisfied across a contiguous
/// run of samples spanning at least `min_ms` of telemetry time.
fn sustained<F>(samples: &[Telemetry], min_ms: i64, pred: F) -> bool
where
    F: Fn(&Telemetry) -> bool,
{
    let mut run_start: Option<i64> = None;
    for s in samples {
        if pred(s) {
            let start = *run_start.get_or_insert(s.ts_unix_ms);
            if s.ts_unix_ms - start >= min_ms {
                return true;
            }
        } else {
            run_start = None;
        }
    }
    false
}

fn excessive_idle(samples: &[Telemetry], now_ms: i64) -> bool {
    // Walk back until we find a non-idle sample; if the most recent
    // non-idle sample is older than the threshold (or there is none in
    // the window), the truck is in excessive idle.
    let last_non_idle = samples.iter().rev().find(|s| s.state != TruckState::Idle as i32);
    let last_idle = samples.iter().rev().find(|s| s.state == TruckState::Idle as i32);
    match (last_non_idle, last_idle) {
        (None, Some(_)) => {
            let span = now_ms - samples[0].ts_unix_ms;
            span >= EXCESSIVE_IDLE_MS
        }
        (Some(non_idle), Some(_)) => {
            let span = now_ms - non_idle.ts_unix_ms;
            span >= EXCESSIVE_IDLE_MS
        }
        _ => false,
    }
}

fn fuel_anomaly(samples: &[Telemetry]) -> bool {
    let last_ts = samples.last().map(|s| s.ts_unix_ms).unwrap_or(0);
    let cutoff = last_ts - FUEL_WINDOW_MS;
    let window: Vec<f32> = samples
        .iter()
        .filter(|s| s.ts_unix_ms >= cutoff)
        .map(|s| s.fuel_pct)
        .collect();
    if window.len() < 2 {
        return false;
    }
    let max = window.iter().cloned().fold(f32::MIN, f32::max);
    let min = window.iter().cloned().fold(f32::MAX, f32::min);
    (max - min) >= FUEL_DROP_PCT
}

fn gps_stale(samples: &[Telemetry], now_ms: i64) -> bool {
    // Stale if no sample with a valid GPS fix in the last GPS_STALE_MS.
    samples
        .iter()
        .rev()
        .find(|s| s.gps.is_some())
        .map(|s| (now_ms - s.ts_unix_ms) >= GPS_STALE_MS)
        .unwrap_or(true)
}

fn stuck(samples: &[Telemetry], now_ms: i64) -> bool {
    // No new sample in N minutes — sim crashed or comms dropped.
    let last_ts = samples.last().map(|s| s.ts_unix_ms).unwrap_or(0);
    (now_ms - last_ts) >= STUCK_NO_TELEMETRY_MS
}

fn load_mismatch(samples: &[Telemetry]) -> bool {
    // Reports Loaded while in a state that should be empty (Returning),
    // or Empty while in a state that should be loaded (Hauling). Quiet
    // disagreements with reality are exactly what dispatchers want to
    // see.
    samples.iter().rev().take(3).any(|s| {
        let state = TruckState::try_from(s.state).unwrap_or(TruckState::Unspecified);
        let load = LoadStatus::try_from(s.load).unwrap_or(LoadStatus::Unspecified);
        match (state, load) {
            (TruckState::Hauling, LoadStatus::Empty) => true,
            (TruckState::Returning, LoadStatus::Loaded) => true,
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(ts: i64) -> Telemetry {
        Telemetry {
            truck_id: "T-1".into(),
            ts_unix_ms: ts,
            gps: Some(fleet_proto::v1::GpsFix {
                lat: 0.0, lon: 0.0, alt_m: 0.0, hdop: 1.0,
            }),
            speed_kmh: 10.0,
            rpm: 1500,
            load: LoadStatus::Empty as i32,
            fuel_pct: 0.8,
            state: TruckState::Idle as i32,
        }
    }

    #[test]
    fn over_rev_fires_after_5s_sustained() {
        let mut s: Vec<_> = (0..=6).map(|i| {
            let mut x = t(i * 1000);
            x.rpm = 2300;
            x
        }).collect();
        // Add an early non-spike sample so a "single brief spike" near-miss
        // can be inserted in the negative test.
        s.insert(0, t(-1000));
        assert!(classify(&s, 7000).contains(&HealthKind::OverRev));
    }

    #[test]
    fn over_rev_does_not_fire_for_single_spike() {
        let mut s = vec![t(0), t(1000), t(2000), t(3000), t(4000)];
        s[2].rpm = 2400; // single sample
        assert!(!classify(&s, 5000).contains(&HealthKind::OverRev));
    }

    #[test]
    fn over_rev_does_not_fire_for_short_burst() {
        // 3s of high RPM — under the 5s threshold.
        let mut s = vec![t(0), t(1000), t(2000), t(3000), t(4000)];
        for i in 1..=3 {
            s[i].rpm = 2400;
        }
        assert!(!classify(&s, 5000).contains(&HealthKind::OverRev));
    }

    #[test]
    fn unsafe_speed_under_load_fires() {
        let s: Vec<_> = (0..=4).map(|i| {
            let mut x = t(i * 1000);
            x.speed_kmh = 50.0;
            x.load = LoadStatus::Loaded as i32;
            x.state = TruckState::Hauling as i32;
            x
        }).collect();
        assert!(classify(&s, 5000).contains(&HealthKind::UnsafeSpeedUnderLoad));
    }

    #[test]
    fn unsafe_speed_does_not_fire_when_empty() {
        let s: Vec<_> = (0..=4).map(|i| {
            let mut x = t(i * 1000);
            x.speed_kmh = 50.0;
            x.load = LoadStatus::Empty as i32;
            x.state = TruckState::Returning as i32;
            x
        }).collect();
        assert!(!classify(&s, 5000).contains(&HealthKind::UnsafeSpeedUnderLoad));
    }

    #[test]
    fn fuel_anomaly_fires_on_5pct_drop_in_60s() {
        let mut s = vec![t(0), t(30_000), t(60_000)];
        s[0].fuel_pct = 0.80;
        s[1].fuel_pct = 0.79;
        s[2].fuel_pct = 0.74; // 6% drop
        assert!(classify(&s, 60_000).contains(&HealthKind::FuelAnomaly));
    }

    #[test]
    fn fuel_anomaly_does_not_fire_on_normal_drain() {
        let mut s = vec![t(0), t(30_000), t(60_000)];
        s[0].fuel_pct = 0.80;
        s[1].fuel_pct = 0.79;
        s[2].fuel_pct = 0.78; // 2% drop — within normal range
        assert!(!classify(&s, 60_000).contains(&HealthKind::FuelAnomaly));
    }

    #[test]
    fn gps_stale_fires_when_last_fix_is_old() {
        let mut s = vec![t(0)];
        s[0].gps = None;
        // last sample is 15s old, no fix
        assert!(classify(&s, 15_000).contains(&HealthKind::GpsStale));
    }

    #[test]
    fn gps_stale_does_not_fire_for_fresh_fix() {
        let s = vec![t(14_000)];
        assert!(!classify(&s, 15_000).contains(&HealthKind::GpsStale));
    }

    #[test]
    fn load_mismatch_fires_on_returning_loaded() {
        let mut s = vec![t(0)];
        s[0].state = TruckState::Returning as i32;
        s[0].load = LoadStatus::Loaded as i32;
        assert!(classify(&s, 0).contains(&HealthKind::LoadMismatch));
    }

    #[test]
    fn empty_window_yields_no_alerts() {
        assert!(classify(&[], 0).is_empty());
    }
}
