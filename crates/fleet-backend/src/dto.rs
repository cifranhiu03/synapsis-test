//! JSON DTOs for browser-facing endpoints.
//!
//! We keep these separate from the protobuf types on purpose: the wire
//! contract between simulator and backend is binary, but the dashboard
//! reads via plain JSON so it stays trivially debuggable with `curl`.
//! Adding `serde` derives directly to generated prost types is possible
//! but couples schema evolution to Serde semantics — we prefer the
//! explicit seam.

use crate::state::TruckSnapshot;
use fleet_proto::v1::{HealthEvent, HealthKind, LoadStatus, Severity, Telemetry, TruckState};
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct GpsDto {
    pub lat: f64,
    pub lon: f64,
    pub alt_m: f64,
    pub hdop: f64,
}

#[derive(Serialize, Clone)]
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
    /// Omitted on history rows (where `received_at` isn't meaningful).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_ms: Option<u64>,
}

impl TruckDto {
    pub fn from_snapshot(snap: &TruckSnapshot) -> Self {
        let mut dto = Self::from_telemetry(&snap.telemetry);
        dto.age_ms = Some(snap.received_at.elapsed().as_millis() as u64);
        dto
    }

    pub fn from_telemetry(t: &Telemetry) -> Self {
        Self {
            truck_id: t.truck_id.clone(),
            ts_unix_ms: t.ts_unix_ms,
            gps: t.gps.as_ref().map(|g| GpsDto {
                lat: g.lat,
                lon: g.lon,
                alt_m: g.alt_m,
                hdop: g.hdop,
            }),
            speed_kmh: t.speed_kmh,
            rpm: t.rpm,
            load: load_label(t.load),
            fuel_pct: t.fuel_pct,
            state: state_label(t.state),
            age_ms: None,
        }
    }
}

#[derive(Serialize, Clone)]
pub struct HealthEventDto {
    pub truck_id: String,
    pub ts_unix_ms: i64,
    pub kind: &'static str,
    pub severity: &'static str,
    pub message: String,
}

impl HealthEventDto {
    pub fn from_event(e: &HealthEvent) -> Self {
        Self {
            truck_id: e.truck_id.clone(),
            ts_unix_ms: e.ts_unix_ms,
            kind: kind_label(e.kind),
            severity: severity_label(e.severity),
            message: e.message.clone(),
        }
    }
}

/// Discriminated union we send over SSE. The `kind` tag matches the
/// dashboard's reducer switch.
#[derive(Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum StreamEvent {
    Snapshot(Vec<TruckDto>),
    Telemetry(TruckDto),
    Health(HealthEventDto),
    /// Tells the client the bus lagged; the client should refetch
    /// `/api/fleet` and resume listening.
    Resync,
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

fn kind_label(v: i32) -> &'static str {
    match HealthKind::try_from(v).unwrap_or(HealthKind::Unspecified) {
        HealthKind::OverRev => "OVER_REV",
        HealthKind::UnsafeSpeedUnderLoad => "UNSAFE_SPEED_UNDER_LOAD",
        HealthKind::ExcessiveIdle => "EXCESSIVE_IDLE",
        HealthKind::FuelAnomaly => "FUEL_ANOMALY",
        HealthKind::GpsStale => "GPS_STALE",
        HealthKind::Stuck => "STUCK",
        HealthKind::LoadMismatch => "LOAD_MISMATCH",
        HealthKind::Unspecified => "UNKNOWN",
    }
}

fn severity_label(v: i32) -> &'static str {
    match Severity::try_from(v).unwrap_or(Severity::Unspecified) {
        Severity::Info => "INFO",
        Severity::Warn => "WARN",
        Severity::Fault => "FAULT",
        Severity::Unspecified => "UNKNOWN",
    }
}
