//! `GET /api/fleet` — current fleet snapshot as JSON.

use axum::extract::State;
use axum::Json;

use crate::dto::TruckDto;
use crate::state::AppState;

pub async fn fleet(State(state): State<AppState>) -> Json<Vec<TruckDto>> {
    let mut out: Vec<TruckDto> = state
        .fleet
        .iter()
        .map(|kv| TruckDto::from_snapshot(kv.value()))
        .collect();
    // Stable order by truck id so the dashboard list doesn't shimmer.
    out.sort_by(|a, b| a.truck_id.cmp(&b.truck_id));
    Json(out)
}
