# Mine Fleet Live Tracker — TODO

## Stack (locked)
- Backend: Rust + Axum + Tokio
- Wire format: Protobuf (prost)
- Push transport: SSE
- Simulator: Rust (cargo workspace)
- Frontend: React + TS + Vite + MapLibre + Zustand
- History storage: in-memory ring buffer
- Bring-up: docker compose

## Repo layout
```
/proto/                  # .proto files — single source of truth
/crates/
  fleet-proto/           # generated Rust types (prost-build)
  fleet-sim/             # simulator binary
  fleet-backend/         # axum service
/web/                    # React + Vite + TS dashboard
/docker-compose.yml
/justfile                # dev convenience
/README.md
```

---

## Day 1 — Scaffolding
- [ ] `cargo new` workspace with `fleet-proto`, `fleet-sim`, `fleet-backend`
- [ ] `proto/fleet.proto` v1: `TruckId`, `TruckState`, `Telemetry`, `HealthEvent`, `FleetUpdate` (oneof)
- [ ] `fleet-proto` build.rs with prost-build, types compile
- [ ] `web/` Vite + React + TS skeleton
- [ ] `docker-compose.yml` stub (backend + sim + web services)
- [ ] `README.md` skeleton with sections: Architecture, Run, Decisions, AI log, Next
- [ ] Commit: scaffolding

## Day 2 — Simulator
- [ ] State machine: `Idle → LoadingQueue → Loading → Hauling → AtCrusher → Dumping → Returning → Idle`
- [ ] Per-truck route polyline + interpolated GPS at 2 Hz
- [ ] Realistic dwell times per state
- [ ] GPS noise injection
- [ ] 5 trucks, each its own tokio task with varied behaviour
- [ ] One truck programmed to misbehave (over-rev, fuel anomaly) for demo
- [ ] POSTs protobuf-encoded `TelemetryFrame` to backend `/ingest`
- [ ] Commit: simulator green

## Day 3 — Backend ingest + state
- [ ] Axum app skeleton, tracing-subscriber, graceful shutdown
- [ ] `POST /ingest` accepts protobuf, validates, updates state
- [ ] `Arc<DashMap<TruckId, TruckState>>` current snapshot
- [ ] `GET /api/fleet` snapshot endpoint
- [ ] Unit tests: state transitions, malformed proto → 400 (no panic)
- [ ] Commit: backend ingest + state

## Day 4 — Streaming + history + health
- [ ] Per-truck `Mutex<VecDeque<Sample>>` ring buffer (~1200 samples)
- [ ] `GET /api/trucks/:id/history?since=...`
- [ ] `tokio::sync::broadcast` channel for `FleetUpdate`
- [ ] `GET /api/stream` SSE; slow clients drop via `Lagged`
- [ ] Health classifier (pure fn over sliding window):
  - [ ] Sustained over-rev: RPM > 2200 for ≥ 5s
  - [ ] Unsafe speed under load: > 40 km/h while Loaded
  - [ ] Excessive idle: Idle > 10 min in working zone
  - [ ] Fuel anomaly: drop > 5% in 60s
  - [ ] GPS dropout: no fix > 10s → Stale (not fault)
- [ ] **Concurrency test**: 50 SSE subscribers + 10 Hz ingest, slow client gets Lagged, healthy clients don't drop
- [ ] **Boundary test**: ring buffer wrap at N+1
- [ ] Commit: streaming + health

## Day 5 — Frontend live dashboard
- [ ] MapLibre map, single GeoJSON source for trucks (not per-marker components)
- [ ] `EventSource` → Zustand store
- [ ] SSE reducer handles snapshot + delta + health events
- [ ] Status-coloured markers
- [ ] Fleet summary panel (counts by state, active alerts)
- [ ] Click marker → drill-down panel (current stats)
- [ ] Reducer unit tests: out-of-order frames, duplicates
- [ ] Commit: dashboard live

## Day 6 — History view + resilience
- [ ] Per-vehicle history view: map trail + speed/state timeline + scrubber
- [ ] Edge-case pass:
  - [ ] GPS dropout shown as Stale, not error
  - [ ] Malformed frames rejected, backend stays up
  - [ ] Simulator restart mid-stream — clients reconnect cleanly
  - [ ] SSE auto-reconnect with `Last-Event-ID` resume
  - [ ] Trucks stuck between state transitions handled
- [ ] Commit: history + resilience

## Day 7 — README + bring-up + final tidy
- [ ] Architecture diagram (one diagram, one paragraph)
- [ ] Run instructions verified on clean docker host
- [ ] Stack decisions with rationale (every non-Rust choice + 10× answer)
- [ ] AI usage log:
  - [ ] What was delegated
  - [ ] What I wrote/heavily rewrote
  - [ ] **One concrete case where AI was wrong** — highest-signal paragraph
- [ ] "What I would do next" — 2–3 conscious omissions
- [ ] Final clean-up: dead code, TODOs, commented-out blocks
- [ ] End-to-end docker compose smoke on fresh checkout
- [ ] Commit: docs + bring-up verified
- [ ] Push to private GitHub repo
- [ ] Invite synapsissoftware10@gmail.com

---

## Deliberately NOT shipping (document in README "Next")
- SQLite/Timescale persistence (10× answer)
- Auth / multi-tenant
- 3D / CesiumJS (stretch — half-finished costs us)
- UDP/QUIC ingest (HTTP easier to defend at this scope)

## Hard fails to avoid
- [ ] Single "initial commit" — commit incrementally each day
- [ ] Reviewer can't bring it up — verify on clean machine Day 7
- [ ] Fake or missing AI usage log
- [ ] Can't explain own code — read every diff before it lands
