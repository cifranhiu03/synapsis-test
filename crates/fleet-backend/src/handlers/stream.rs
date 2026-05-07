//! `GET /api/stream` — SSE fanout of fleet updates.
//!
//! Wire shape: each SSE message body is a JSON `StreamEvent` (see `dto`).
//! Connection lifecycle:
//!   1. On connect, send a `snapshot` event reflecting current state so a
//!      late-joining client doesn't need a separate snapshot fetch.
//!   2. Subscribe to the broadcast bus and forward `telemetry` / `health`.
//!   3. On `Lagged(n)` (slow client behind the bus), emit `resync` —
//!      the client refetches `/api/fleet` to recover, then keeps listening.
//!   4. Heartbeat every 15s so intermediaries don't time out idle conns.

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use futures::stream::{Stream, StreamExt};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;

use crate::dto::{HealthEventDto, StreamEvent, TruckDto};
use crate::state::AppState;
use fleet_proto::v1::fleet_update::Payload;

pub async fn stream(State(state): State<AppState>) -> impl IntoResponse {
    let rx = state.tx.subscribe();

    // Build the initial snapshot synchronously — it reads only DashMap
    // shards and can't block.
    let mut snap: Vec<TruckDto> = state
        .fleet
        .iter()
        .map(|kv| TruckDto::from_snapshot(kv.value()))
        .collect();
    snap.sort_by(|a, b| a.truck_id.cmp(&b.truck_id));
    let initial = futures::stream::once(async move {
        Ok::<_, Infallible>(encode(StreamEvent::Snapshot(snap)))
    });

    let live = BroadcastStream::new(rx).filter_map(|item| async move {
        match item {
            Ok(update) => match update.payload {
                Some(Payload::Telemetry(t)) => Some(Ok(encode(StreamEvent::Telemetry(
                    TruckDto::from_telemetry(&t),
                )))),
                Some(Payload::Health(h)) => Some(Ok(encode(StreamEvent::Health(
                    HealthEventDto::from_event(&h),
                )))),
                Some(Payload::Snapshot(_)) | None => None,
            },
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(_)) => {
                Some(Ok(encode(StreamEvent::Resync)))
            }
        }
    });

    let stream = initial.chain(live);
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

fn encode(event: StreamEvent) -> Event {
    let body = serde_json::to_string(&event).expect("event serialises");
    Event::default().data(body)
}

// Hint to the type checker that the stream item type is what axum::Sse expects.
#[allow(dead_code)]
fn _assert_stream_type<S>(_: S)
where
    S: Stream<Item = Result<Event, Infallible>>,
{
}
