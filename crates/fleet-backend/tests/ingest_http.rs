//! Integration tests at the HTTP boundary — calls go through the real
//! axum router, not handler functions directly. This is what catches
//! routing typos and middleware misconfiguration.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use fleet_backend::{app::router, state::AppState};
use fleet_proto::v1::{LoadStatus, Telemetry, TruckState};
use prost::Message;
use tower::ServiceExt;

fn frame(id: &str) -> Telemetry {
    Telemetry {
        truck_id: id.into(),
        ts_unix_ms: 1,
        gps: None,
        speed_kmh: 12.0,
        rpm: 1500,
        load: LoadStatus::Empty as i32,
        fuel_pct: 0.7,
        state: TruckState::Hauling as i32,
    }
}

#[tokio::test]
async fn happy_ingest_then_snapshot_roundtrip() {
    let state = AppState::new();
    let app = router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/ingest")
        .header("content-type", "application/x-protobuf")
        .body(Body::from(frame("T-01").encode_to_vec()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);

    let req = Request::builder().uri("/api/fleet").body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = to_bytes(res.into_body(), 64 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json[0]["truck_id"], "T-01");
    assert_eq!(json[0]["state"], "HAULING");
}

#[tokio::test]
async fn malformed_body_returns_400_and_state_unchanged() {
    let state = AppState::new();
    let app = router(state.clone());

    let req = Request::builder()
        .method("POST")
        .uri("/ingest")
        .body(Body::from(&b"garbage"[..]))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(state.fleet.is_empty());
}

#[tokio::test]
async fn healthz_is_200() {
    let app = router(AppState::new());
    let req = Request::builder().uri("/healthz").body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
