//! Concurrency contract for the broadcast bus.
//!
//! 50 fast subscribers + 1 deliberately slow subscriber + an ingest
//! producer running at 10 Hz for ~5s of telemetry frames.
//!
//! Asserts:
//!   * fast subscribers receive *every* telemetry frame they witnessed,
//!     no holes (counts match the producer's send count).
//!   * the slow subscriber sees `Lagged` at least once (proves the bus
//!     is bounded and the SSE handler's resync path is reachable).
//!
//! This is the spine the SSE endpoint relies on; integration-testing
//! through hyper would be slower, flakier, and cover the same contract.

use fleet_backend::state::{AppState, BROADCAST_CAPACITY};
use fleet_proto::v1::{fleet_update::Payload, LoadStatus, Telemetry, TruckState};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;

fn frame(i: i64) -> Telemetry {
    Telemetry {
        truck_id: format!("T-{:02}", (i % 5) + 1),
        ts_unix_ms: i,
        gps: None,
        speed_kmh: 12.0,
        rpm: 1500,
        load: LoadStatus::Empty as i32,
        fuel_pct: 0.7,
        state: TruckState::Hauling as i32,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fast_clients_see_all_frames_slow_client_lags() {
    let state = AppState::new();

    let total: usize = 500; // ~5s @ 10 Hz, well above BROADCAST_CAPACITY
    let fast_count = 50;

    // Spawn fast subscribers.
    let received = Arc::new(AtomicUsize::new(0));
    let mut fast_handles = Vec::new();
    for _ in 0..fast_count {
        let mut rx = state.tx.subscribe();
        let received = received.clone();
        fast_handles.push(tokio::spawn(async move {
            let mut got = 0usize;
            let mut lagged = false;
            loop {
                match rx.recv().await {
                    Ok(_) => {
                        got += 1;
                        received.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(RecvError::Lagged(_)) => {
                        lagged = true;
                    }
                    Err(RecvError::Closed) => break,
                }
            }
            (got, lagged)
        }));
    }

    // Slow subscriber: holds its rx but never reads. Once the bus
    // overflows, it will see Lagged on its first recv.
    let slow_rx = state.tx.subscribe();

    // Producer.
    let producer_state = state.clone();
    let producer = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(10));
        for i in 0..total {
            interval.tick().await;
            producer_state.apply_telemetry(frame(i as i64));
        }
    });

    producer.await.unwrap();

    // Drop the producer's senders side by closing — drop state so when
    // the function ends subscribers see Closed. We've kept the original
    // `state` alive for the producer above, so close by dropping last
    // owner here.
    drop(state);

    let mut fast_results = Vec::new();
    for h in fast_handles {
        fast_results.push(h.await.unwrap());
    }

    // No fast subscriber should have lagged: BROADCAST_CAPACITY is sized
    // generously above 5s of fanout for our tick rate. If this becomes
    // flaky, raise BROADCAST_CAPACITY or lower the producer rate — the
    // test is documenting the contract, not papering over it.
    for (got, lagged) in &fast_results {
        assert!(
            !lagged,
            "fast subscriber lagged unexpectedly (got {got}, expected {total})"
        );
        assert_eq!(*got, total, "fast subscriber missed frames");
    }

    // Slow subscriber: now drain it. We expect a Lagged before any Ok.
    let mut slow_rx = slow_rx;
    let mut saw_lagged = false;
    while let Ok(item) = tokio::time::timeout(Duration::from_millis(100), slow_rx.recv()).await {
        match item {
            Err(RecvError::Lagged(_)) => {
                saw_lagged = true;
                break;
            }
            Err(RecvError::Closed) => break,
            Ok(_) => continue,
        }
    }
    assert!(
        saw_lagged,
        "slow subscriber never lagged — bus is unbounded or test sent too few frames \
         (BROADCAST_CAPACITY={BROADCAST_CAPACITY})"
    );
}

#[tokio::test]
async fn history_ring_buffer_evicts_oldest() {
    use fleet_backend::state::HISTORY_CAPACITY;
    let state = AppState::new();
    let n = HISTORY_CAPACITY + 5;
    for i in 0..n {
        let mut f = frame(i as i64);
        f.truck_id = "T-01".into();
        state.apply_telemetry(f);
    }
    let hist = state.history_since("T-01", 0);
    assert_eq!(hist.len(), HISTORY_CAPACITY);
    assert_eq!(hist.first().unwrap().ts_unix_ms, 5);
    assert_eq!(hist.last().unwrap().ts_unix_ms, n as i64 - 1);
}
