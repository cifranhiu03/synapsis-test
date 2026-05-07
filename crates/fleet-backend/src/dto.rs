//! JSON DTOs for browser-facing endpoints.
//!
//! We keep these separate from the protobuf types on purpose: the wire
//! contract between simulator and backend is binary, but the dashboard
//! reads the snapshot via plain JSON so it stays trivially debuggable
//! with `curl`. Adding `serde` derives directly to generated prost types
//! is possible via `type_attribute`, but it couples schema evolution to
//! Serde semantics (e.g., oneof tagging) and we prefer the explicit seam.

use crate::state::TruckSnapshot;
use fleet_proto::v1::{LoadStatus, TruckState};
use serde::Serialize;

#[derive(Serialize)]
pub struct GpsDto {
    pub lat: f64,
    pub lon: f64,
    pub alt_m: f64,
    pub hdop: f64,
}

#[derive(Serialize)]
pub struct TruckDto {
    pub truck_id: String,
    pub ts_unix_ms: i64,
    pub gps: Option<GpsDto>,
    pub speed_kmh: f64,
    pub rpm: u32,
    pub load: &'static str,
    pub fuel_pct: f32,
    pub state: &'static str,
    /// Age of the latest fix in milliseconds, computed at response time.
    pub age_ms: u64,
}

impl TruckDto {
    pub fn from_snapshot(snap: &TruckSnapshot) -> Self {
        let t = &snap.telemetry;
        let gps = t.gps.as_ref().map(|g| GpsDto {
            lat: g.lat,
            lon: g.lon,
            alt_m: g.alt_m,
            hdop: g.hdop,
        });
        Self {
            truck_id: t.truck_id.clone(),
            ts_unix_ms: t.ts_unix_ms,
            gps,
            speed_kmh: t.speed_kmh,
            rpm: t.rpm,
            load: load_label(t.load),
            fuel_pct: t.fuel_pct,
            state: state_label(t.state),
            age_ms: snap.received_at.elapsed().as_millis() as u64,
        }
    }
}

fn load_label(v: i32) -> &'static str {
    match LoadStatus::try_from(v).unwrap_or(LoadStatus::Unspecified) {
        LoadStatus::Empty => "EMPTY",
        LoadStatus::Loaded => "LOADED",
        LoadStatus::Unspecified => "UNKNOWN",
    }
}

fn state_label(v: i32) -> &'static str {
    match TruckState::try_from(v).unwrap_or(TruckState::Unspecified) {
        TruckState::Idle => "IDLE",
        TruckState::LoadingQueue => "LOADING_QUEUE",
        TruckState::Loading => "LOADING",
        TruckState::Hauling => "HAULING",
        TruckState::AtCrusher => "AT_CRUSHER",
        TruckState::Dumping => "DUMPING",
        TruckState::Returning => "RETURNING",
        TruckState::Unspecified => "UNKNOWN",
    }
}
