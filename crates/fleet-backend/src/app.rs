//! Router wiring. Pulled out of `main.rs` so integration tests can build
//! the router without binding a port.

use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/ingest", post(handlers::ingest::ingest))
        .route("/api/fleet", get(handlers::snapshot::fleet))
        // CORS is permissive for the demo — the dashboard is served from
        // the same origin via nginx in production-style runs, but local
        // `npm run dev` hits the backend directly.
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}
