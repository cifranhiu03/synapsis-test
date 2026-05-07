//! `GET /api/trucks/:id/history?since_ms=...` — slice of the per-truck
//! ring buffer. Returns JSON ordered by ascending timestamp. Unknown
//! truck → 404 (not 200 with empty array — distinguishing "no truck" from
//! "no recent samples" is meaningful to the caller).

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::dto::TruckDto;
use crate::error::AppError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct HistoryQuery {
    /// Lower bound on `ts_unix_ms`. Default 0 returns the full buffer.
    #[serde(default)]
    pub since_ms: i64,
}

pub async fn history(
    State(state): State<AppState>,
    Path(truck_id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<TruckDto>>, AppError> {
    if !state.fleet.contains_key(&truck_id) {
        return Err(AppError::NotFound);
    }
    let rows = state
        .history_since(&truck_id, q.since_ms)
        .iter()
        .map(TruckDto::from_telemetry)
        .collect();
    Ok(Json(rows))
}
