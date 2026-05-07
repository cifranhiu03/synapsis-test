# Mine Fleet Live Tracker — Detailed Task Breakdown

Each task: **what · where · how · done-when**. Check off as you ship. Commit at every `---` checkpoint.

---

## D1.1 — Cargo workspace
- **What**: empty 3-crate workspace builds.
- **Where**: `/Cargo.toml`, `/crates/{fleet-proto,fleet-sim,fleet-backend}/Cargo.toml`
- **How**:
  - Root `Cargo.toml` with `[workspace] members = ["crates/*"]`, shared `[workspace.dependencies]` for tokio, prost, tracing, anyhow, serde.
  - Pin Rust toolchain via `rust-toolchain.toml` (stable, e.g. 1.83).
- **Done when**: `cargo build --workspace` succeeds with no warnings.

## D1.2 — Proto schema v1
- **What**: single source of truth for wire types.
- **Where**: `/proto/fleet.proto`
- **How**:
  - `syntax = "proto3"; package fleet.v1;`
  - Messages: `TruckId` (string), `GpsFix { lat, lon, alt, hdop }`, `Telemetry { truck_id, ts_unix_ms, gps, speed_kmh, rpm, load (enum Empty/Loaded/Unknown), fuel_pct, state (enum) }`
  - `enum TruckState { UNKNOWN=0; IDLE=1; LOADING_QUEUE=2; LOADING=3; HAULING=4; AT_CRUSHER=5; DUMPING=6; RETURNING=7; }`
  - `HealthEvent { truck_id, ts_unix_ms, kind (enum), severity (enum), message }`
  - `FleetUpdate { oneof payload { Telemetry t; HealthEvent h; FleetSnapshot snap; } }`
  - Reserve field numbers 50–99 for future use; document in proto comments.
- **Done when**: file lints with `protoc --lint` (or buf), all enums have `_UNSPECIFIED = 0`.

## D1.3 — fleet-proto crate
- **What**: Rust types generated from proto at build time.
- **Where**: `/crates/fleet-proto/{Cargo.toml,build.rs,src/lib.rs}`
- **How**: `prost-build` in `build.rs`, re-export generated module from `lib.rs`.
- **Done when**: `use fleet_proto::v1::Telemetry;` compiles in another crate.

## D1.4 — Web skeleton
- **Where**: `/web`
- **How**: `npm create vite@latest web -- --template react-ts`; add `maplibre-gl`, `zustand`, `vitest`, `@testing-library/react`.
- **Done when**: `npm run dev` shows default page; `npm test` runs zero tests successfully.

## D1.5 — docker-compose stub
- **Where**: `/docker-compose.yml`, `/crates/fleet-backend/Dockerfile`, `/crates/fleet-sim/Dockerfile`, `/web/Dockerfile`
- **How**: 3 services on a shared network. Backend exposes 8080, web exposes 5173. Sim depends on backend. Multi-stage Rust build (cargo-chef for cache).
- **Done when**: `docker compose build` succeeds; services start (even if they do nothing useful).

## D1.6 — README skeleton
- **Where**: `/README.md`
- **How**: empty headings: Architecture, Run, Stack Decisions, AI Usage Log, What's Next, Tests.
- **Done when**: file exists with TOC.

> **Commit checkpoint D1**: `chore: workspace + proto schema + compose skeleton`

---

## D2.1 — Truck state machine
- **Where**: `/crates/fleet-sim/src/state.rs`
- **How**:
  - `enum SimState` mirrors proto enum.
  - `Transition { from, to, min_dwell_s, max_dwell_s }` table.
  - `fn next(state, rng) -> (SimState, Duration)`.
- **Done when**: unit test cycles a truck through full loop in < 30 simulated minutes.

## D2.2 — Route + GPS interpolation
- **Where**: `/crates/fleet-sim/src/route.rs`
- **How**:
  - Hard-code 5 polylines (load zone → haul road → crusher → return) as `Vec<(f64,f64)>`. Use a real-ish open-pit area (e.g., near Kalimantan coords) for plausibility.
  - `fn position_at(route, progress: f32) -> GpsFix` — linear interp between vertices.
  - GPS noise: gaussian σ ≈ 2m, occasional dropout (1% of ticks → emit hdop=99).
- **Done when**: ticking a truck for 60s produces a smooth path with one synthetic dropout.

## D2.3 — Telemetry derivation
- **Where**: `/crates/fleet-sim/src/truck.rs`
- **How**: per-tick produce `Telemetry`: speed from progress delta, RPM derived from speed + load, fuel monotonically decreasing, load tied to state (LOADING→Loaded after dwell).
- **Done when**: serialized telemetry passes a hand-checked sanity test.

## D2.4 — Misbehaving truck
- **Where**: same
- **How**: truck #3 occasionally over-revs (RPM spike for 6s) every ~3 minutes; truck #5 drops fuel by 8% in 30s once during run.
- **Done when**: log shows the events firing.

## D2.5 — Tokio task per truck + ingest client
- **Where**: `/crates/fleet-sim/src/main.rs`
- **How**:
  - `tokio::spawn` one task per truck at 2 Hz.
  - `reqwest` client POSTs `application/x-protobuf` bytes to `BACKEND_URL/ingest`.
  - Backoff on connection error; never panic.
  - Config via env: `BACKEND_URL`, `TICK_HZ`, `TRUCK_COUNT`.
- **Done when**: running sim against a `nc -l 8080` shows POSTs flowing.

> **Commit checkpoint D2**: `feat(sim): 5-truck simulator with state machine and ingest client`

---

## D3.1 — Axum app skeleton
- **Where**: `/crates/fleet-backend/src/{main.rs,app.rs,error.rs}`
- **How**:
  - `tracing-subscriber` JSON to stderr.
  - `tokio::signal::ctrl_c` graceful shutdown that closes broadcast channel.
  - Typed `AppError` implementing `IntoResponse`.
- **Done when**: server starts, `/healthz` returns 200.

## D3.2 — AppState
- **Where**: `/crates/fleet-backend/src/state.rs`
- **How**:
  - `struct AppState { fleet: Arc<DashMap<String, TruckState>>, history: Arc<DashMap<String, Mutex<VecDeque<Sample>>>>, tx: broadcast::Sender<FleetUpdate> }`
  - `TruckState` is the in-memory aggregate (latest telemetry + derived flags).
- **Done when**: state constructible, accessible from handlers via `State(...)`.

## D3.3 — `POST /ingest`
- **Where**: `/crates/fleet-backend/src/handlers/ingest.rs`
- **How**:
  - Read body as bytes, decode `Telemetry` via prost.
  - On decode error → 400 with brief reason, log at warn (not error).
  - On success: update `fleet`, push to `history` ring buffer (cap 1200), `tx.send(FleetUpdate::telemetry(...))`.
  - Run health classifier on the truck's window; if event → `tx.send(FleetUpdate::health(...))`.
- **Done when**: integration test posts proto, snapshot reflects it, malformed body returns 400 cleanly.

## D3.4 — `GET /api/fleet`
- **Where**: `/crates/fleet-backend/src/handlers/snapshot.rs`
- **How**: serialize current `DashMap` to JSON (browser-friendly). Document choice in code comment + README.
- **Done when**: returns `[]` when empty, populated array after ingest.

## D3.5 — Unit tests
- **Where**: `/crates/fleet-backend/src/state.rs` (mod tests)
- **How**: state transition tests, history wrap test (push 1201 → len = 1200, oldest gone).
- **Done when**: `cargo test -p fleet-backend` passes.

> **Commit checkpoint D3**: `feat(backend): ingest, snapshot, in-memory state`

---

## D4.1 — Health classifier
- **Where**: `/crates/fleet-backend/src/health.rs`
- **How**: pure `fn classify(window: &[Sample], now: Instant) -> Vec<HealthEvent>`. Rules:
  - Sustained over-rev: any contiguous ≥5s with RPM>2200.
  - Unsafe speed under load: speed>40 while Loaded.
  - Excessive idle: state Idle for >10 min in working bbox.
  - Fuel anomaly: max-min > 5% in last 60s window.
  - Stale GPS: last fix age > 10s — emits `Stale` (severity Info), not Fault.
- **Done when**: table-driven tests cover each rule + each "shouldn't fire" near-miss.

## D4.2 — Ring buffer history endpoint
- **Where**: `/crates/fleet-backend/src/handlers/history.rs`
- **How**: `GET /api/trucks/:id/history?since_ms=`. Filter ring buffer by ts. Return JSON `[Sample]`.
- **Done when**: returns expected slice for a known buffer state in test.

## D4.3 — SSE stream
- **Where**: `/crates/fleet-backend/src/handlers/stream.rs`
- **How**:
  - `GET /api/stream` returns `Sse<impl Stream>`.
  - On connect: send initial snapshot event, then subscribe to broadcast.
  - Use `BroadcastStream`; on `Err(Lagged(n))` → send a `resync` SSE event telling client to refetch snapshot, then continue.
  - Heartbeat every 15s via `KeepAlive`.
  - Use SSE event IDs (monotonic counter) for `Last-Event-ID` resume best-effort.
- **Done when**: `curl -N localhost:8080/api/stream` shows live frames.

## D4.4 — Concurrency test
- **Where**: `/crates/fleet-backend/tests/concurrency.rs`
- **How**:
  - Spawn server.
  - 50 SSE subscribers, 1 deliberately slow (sleeps between reads).
  - Drive ingest at 10 Hz for 5s.
  - Assert: all 49 fast clients receive every health event; slow client sees a `resync` event (Lagged path).
- **Done when**: test passes 10 runs in a row (`cargo test -- --test-threads=1`).

## D4.5 — Failure-path test
- **Where**: `/crates/fleet-backend/tests/ingest_malformed.rs`
- **How**: POST random bytes, expect 400; assert state unchanged; server still healthy.
- **Done when**: test green.

> **Commit checkpoint D4**: `feat(backend): SSE fanout, history, health classifier with tests`

---

## D5.1 — SSE client + store
- **Where**: `/web/src/store/fleet.ts`
- **How**:
  - Zustand store: `trucks: Map<id, TruckState>`, `alerts: HealthEvent[]`, `connected: boolean`.
  - `connect()` opens `EventSource('/api/stream')`, dispatches reducer per event type.
  - Reducer is a pure function exported separately for testing.
  - On `resync` event: refetch `/api/fleet`, replace map.
- **Done when**: opening dev server populates store from live backend.

## D5.2 — MapLibre map
- **Where**: `/web/src/components/FleetMap.tsx`
- **How**:
  - Single map instance in a ref. **One** GeoJSON source `"trucks"` with a circle layer styled by `state` property.
  - Subscribe to store; on change, call `source.setData(featureCollection)`. **No React-rendered markers.** This is the re-render-storm avoidance — comment the file with a one-liner pointing to README §Performance.
  - Click handler → `setSelected(id)` in store.
- **Done when**: 5 markers appear, move smoothly, change colour on state change.

## D5.3 — Fleet panel
- **Where**: `/web/src/components/FleetPanel.tsx`
- **How**: counts by state, list of active alerts (severity-coloured), click to focus map on truck.
- **Done when**: panel updates without flicker as state changes.

## D5.4 — Drill-down panel
- **Where**: `/web/src/components/TruckDetail.tsx`
- **How**: shown when `selected` set; current stats (speed, RPM, fuel, load, last fix age).
- **Done when**: opens/closes cleanly, values live-update.

## D5.5 — Reducer tests
- **Where**: `/web/src/store/fleet.test.ts`
- **How**: vitest cases:
  - Out-of-order frames (older ts arrives after newer) → newer wins.
  - Duplicate frame → idempotent.
  - Health event for unknown truck → buffered or ignored without throwing.
- **Done when**: `npm test` passes.

> **Commit checkpoint D5**: `feat(web): live dashboard with map, panel, drill-down`

---

## D6.1 — History view
- **Where**: `/web/src/components/HistoryView.tsx`
- **How**:
  - Triggered from drill-down ("View history").
  - Fetch `/api/trucks/:id/history?since_ms=...` (last 10 min).
  - Render: map trail (line layer), speed timeline (uPlot), state-band strip, scrubber input.
  - Scrubber updates a "ghost marker" on the map at that timestamp.
- **Done when**: scrubbing updates marker + highlights timeline cursor smoothly.

## D6.2 — Resilience pass
- [ ] Kill simulator mid-run — backend stays up, dashboard shows trucks going Stale after 10s.
- [ ] Restart simulator — dashboard recovers without page reload.
- [ ] Kill backend — frontend shows disconnected state, auto-reconnects.
- [ ] Send 1MB junk to `/ingest` — 400, no OOM, no panic.
- [ ] Open 20 dashboard tabs — no degradation on first 19.

## D6.3 — Domain edge cases (document each in README §Edge Cases)
- [ ] GPS dropout shown as Stale, not as a fault — dispatcher distinction.
- [ ] Sensor noise (single-sample RPM spike) does NOT trigger over-rev — sustained-window rule covers this.
- [ ] Truck stuck between states (no telemetry change for >2 min) → surfaced as `Stuck` health event.
- [ ] Load telemetry disagrees with state (Loaded reported during Returning-empty) → `LoadMismatch` event.

> **Commit checkpoint D6**: `feat(web): history view + resilience hardening`

---

## D7.1 — README polish
- [ ] **Architecture** section: one Mermaid diagram (sim → backend → SSE → web), one paragraph of data flow.
- [ ] **Run** section: prerequisites (Docker ≥ 24, ports 8080/5173 free), exact `docker compose up --build` command, expected output, where to open dashboard.
- [ ] **Stack Decisions** table: Axum, Protobuf, SSE, Rust sim, MapLibre, Zustand, in-memory state, docker compose. Each row: choice / why beats alternative / 10× answer.
- [ ] **AI Usage Log**:
  - What was delegated.
  - What was hand-written / heavily rewritten.
  - **One concrete "AI was wrong" case** (real one, from your dev log).
- [ ] **What's Next**: SQLite/Timescale persistence, auth, 3D/Cesium, UDP/QUIC ingest — and *why each was deliberately deferred*.
- [ ] **Edge Cases** section: bullet list from D6.3 with rationale.

## D7.2 — Bring-up verification
- [ ] On a clean directory: `git clone … && docker compose up --build`.
- [ ] Open dashboard URL → 5 trucks visible within 30s.
- [ ] Trigger one alert (truck #3 over-rev) — visible in panel.
- [ ] Document any prerequisite gotchas discovered.

## D7.3 — Final tidy
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo fmt --all --check` clean.
- [ ] `cd web && npm run lint && npm run typecheck` clean.
- [ ] No `TODO`, `FIXME`, `dbg!`, `console.log` left in code.
- [ ] No commented-out blocks.
- [ ] Every non-trivial module has a module-level doc comment explaining its job.

## D7.4 — Ship
- [ ] Push to private GitHub repo.
- [ ] Invite `synapsissoftware10@gmail.com` (Reporter access).
- [ ] Verify repo loads, README renders, compose still works from a fresh clone.

> **Commit checkpoint D7**: `docs: README, AI log, decision rationale; verified bring-up`

---

## Cross-cutting checklist (review before submitting)
- [ ] No "initial commit" dump — commits span the week.
- [ ] Commit messages explain *why* not *what*.
- [ ] Every stack choice defended in README.
- [ ] AI log includes a real wrong-AI moment.
- [ ] You can walk through every file on a whiteboard. If not, rewrite or delete.
- [ ] All deferred items listed in "What's Next" with rationale.
