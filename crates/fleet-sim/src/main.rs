//! Fleet simulator entrypoint.
//!
//! Spawns one tokio task per truck; each task ticks at `TICK_HZ`, builds a
//! protobuf telemetry frame, and POSTs it to the backend's `/ingest`
//! endpoint. Network errors back off exponentially up to a small ceiling
//! so a slow-starting backend doesn't wedge the simulator and a transient
//! flap doesn't spam the log.

mod route;
mod state;
mod truck;

use anyhow::{Context, Result};
use fleet_proto::v1::Telemetry;
use prost::Message;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::{interval, MissedTickBehavior};
use tracing::{info, warn};

use crate::route::fleet_routes;
use crate::truck::{FaultPlan, Truck};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let backend_url = std::env::var("BACKEND_URL").unwrap_or_else(|_| "http://localhost:8080".into());
    let tick_hz: f64 = std::env::var("TICK_HZ")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0);
    let truck_count: usize = std::env::var("TRUCK_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    info!(%backend_url, tick_hz, truck_count, "starting simulator");

    let routes = fleet_routes();
    let mut handles = Vec::new();
    for i in 0..truck_count {
        let id = format!("T-{:02}", i + 1);
        let route = routes[i % routes.len()].clone();
        let fault = fault_plan_for(i);
        let backend = backend_url.clone();
        let h = tokio::spawn(run_truck(id, route, fault, backend, tick_hz));
        handles.push(h);
    }

    // If any task panics we want the whole sim to exit non-zero so the
    // container restarts under compose's `restart: unless-stopped`.
    for h in handles {
        h.await.context("truck task panicked")??;
    }
    Ok(())
}

/// Per-truck fault wiring. Kept as a small table so it's obvious at a
/// glance which truck does what; the alternative (random seeds) would
/// produce different demos on every run.
fn fault_plan_for(idx: usize) -> FaultPlan {
    match idx {
        // T-03: periodic over-rev burst — exercises the over-rev classifier.
        2 => FaultPlan {
            over_rev_every: Some(Duration::from_secs(180)),
            over_rev_for: Duration::from_secs(8),
            gps_dropout_chance: 0.005,
            ..Default::default()
        },
        // T-05: a one-shot fuel anomaly 90s in — exercises the fuel rule.
        4 => FaultPlan {
            fuel_drop_at: Some(Duration::from_secs(90)),
            fuel_drop_pct: 0.08,
            gps_dropout_chance: 0.005,
            ..Default::default()
        },
        // Everyone else: occasional GPS dropout only.
        _ => FaultPlan {
            gps_dropout_chance: 0.005,
            ..Default::default()
        },
    }
}

async fn run_truck(
    id: String,
    route: route::Route,
    fault: FaultPlan,
    backend_url: String,
    tick_hz: f64,
) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;

    // Seed per-truck so the same truck behaves identically across runs but
    // different trucks diverge — easier to reproduce a bug if it surfaces.
    let mut rng = StdRng::seed_from_u64(hash_id(&id));
    let mut truck = Truck::new(id.clone(), route, fault, &mut rng);

    let period = Duration::from_secs_f64(1.0 / tick_hz.max(0.1));
    let mut tick = interval(period);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut last = Instant::now();
    let mut backoff = Duration::from_millis(250);
    let max_backoff = Duration::from_secs(5);
    let ingest_url = format!("{}/ingest", backend_url.trim_end_matches('/'));

    loop {
        tick.tick().await;
        let now = Instant::now();
        let dt = now.duration_since(last);
        last = now;

        truck.step(now, dt, &mut rng);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let frame: Telemetry = truck.telemetry(now, ts, &mut rng);
        let bytes = frame.encode_to_vec();

        match client
            .post(&ingest_url)
            .header("content-type", "application/x-protobuf")
            .body(bytes)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                backoff = Duration::from_millis(250);
            }
            Ok(resp) => {
                warn!(truck = %id, status = %resp.status(), "ingest non-2xx");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(max_backoff);
            }
            Err(e) => {
                warn!(truck = %id, error = %e, "ingest failed");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(max_backoff);
            }
        }
    }
}

fn hash_id(id: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    h.finish()
}
