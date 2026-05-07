//! `POST /ingest` — accept one telemetry frame.
//!
//! The simulator (and any future sources) speak protobuf on the wire.
//! Decoding errors return 400 with a short reason; the body is logged at
//! warn level rather than error so a misbehaving client can't spam the
//! error log of an otherwise healthy service.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use fleet_proto::v1::Telemetry;
use prost::Message;

use crate::error::AppError;
use crate::state::AppState;

const MAX_FRAME_BYTES: usize = 64 * 1024;

pub async fn ingest(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    if body.len() > MAX_FRAME_BYTES {
        return Err(AppError::BadRequest(format!(
            "frame too large: {} bytes (max {})",
            body.len(),
            MAX_FRAME_BYTES
        )));
    }

    let frame = Telemetry::decode(body.as_ref())
        .map_err(|e| AppError::BadRequest(format!("decode failed: {e}")))?;

    if frame.truck_id.is_empty() {
        return Err(AppError::BadRequest("missing truck_id".into()));
    }
    // `state` 0 (UNSPECIFIED) means the producer didn't set it. Reject so
    // bugs in producers surface immediately rather than as ghost trucks.
    if frame.state == 0 {
        return Err(AppError::BadRequest("missing state".into()));
    }

    state.apply_telemetry(frame);
    Ok(StatusCode::ACCEPTED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use fleet_proto::v1::{LoadStatus, TruckState};

    fn good_frame() -> Telemetry {
        Telemetry {
            truck_id: "T-01".into(),
            ts_unix_ms: 1,
            gps: None,
            speed_kmh: 10.0,
            rpm: 1500,
            load: LoadStatus::Empty as i32,
            fuel_pct: 0.8,
            state: TruckState::Idle as i32,
        }
    }

    #[tokio::test]
    async fn happy_path_returns_202_and_updates_state() {
        let app = AppState::new();
        let body = Bytes::from(good_frame().encode_to_vec());
        let res = ingest(State(app.clone()), body).await.unwrap();
        assert_eq!(res, StatusCode::ACCEPTED);
        assert!(app.fleet.contains_key("T-01"));
    }

    #[tokio::test]
    async fn malformed_body_returns_bad_request() {
        let app = AppState::new();
        let res = ingest(State(app.clone()), Bytes::from_static(b"not a protobuf")).await;
        assert!(matches!(res, Err(AppError::BadRequest(_))));
        assert!(app.fleet.is_empty(), "state must not change on decode failure");
    }

    #[tokio::test]
    async fn missing_truck_id_returns_bad_request() {
        let app = AppState::new();
        let mut t = good_frame();
        t.truck_id = String::new();
        let res = ingest(State(app.clone()), Bytes::from(t.encode_to_vec())).await;
        assert!(matches!(res, Err(AppError::BadRequest(_))));
    }

    #[tokio::test]
    async fn oversized_body_returns_bad_request() {
        let app = AppState::new();
        let big = vec![0u8; MAX_FRAME_BYTES + 1];
        let res = ingest(State(app), Bytes::from(big)).await;
        assert!(matches!(res, Err(AppError::BadRequest(_))));
    }
}
