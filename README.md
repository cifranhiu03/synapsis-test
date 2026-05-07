# Mine Fleet Live Tracker

Real-time dashboard for an open-pit mine dispatcher: 5 simulated haul trucks,
live telemetry, health alerts, per-vehicle history. Built for the Synapsis
Sr. Full-Stack Software Engineer take-home.

## Architecture

```
                                                 ┌─────────────────┐
  ┌────────────┐   POST /ingest    ┌──────────┐  │ broadcast bus    │
  │ fleet-sim  │ ───protobuf────▶  │ ingest   │─▶│ (tokio::broadcast)│
  │ (5 tasks)  │                   │ handler  │  └────────┬─────────┘
  └────────────┘                   └────┬─────┘           │
                                        │                 │ SSE JSON
                              ┌─────────▼─────────┐       │
                              │ DashMap<id,state> │       ▼
                              │ + per-truck ring  │  ┌─────────────┐
                              │ buffer (1200)     │  │ web (React  │
                              └────────┬──────────┘  │  + MapLibre │
                                       │ JSON         │  + Zustand) │
                              GET /api/fleet,         └─────────────┘
                              /history, /healthz
```

One process per concern: simulator, backend, web. The simulator pushes
protobuf-encoded `Telemetry` frames at 2 Hz over HTTP. The backend decodes,
updates a single in-memory snapshot (`DashMap`) and a per-truck ring buffer,
runs the health classifier over the recent window, and fans the resulting
`FleetUpdate` out to every SSE subscriber via a `tokio::broadcast` channel.
The web client opens one `EventSource`, hydrates a Zustand store, and drives
a single GeoJSON source on a MapLibre map — no per-marker React components.

## How to run it

Prerequisites: Docker ≥ 24, ports `5173` and `8080` free.

```sh
docker compose up --build
```

Open <http://localhost:5173>. Five trucks should appear on the map within
~30 seconds; truck `T-03` is programmed to over-rev periodically and
truck `T-05` will fire a fuel-anomaly alert once during the run.

## Stack decisions

Every choice outside "Rust backend" defended with **why this beats the
alternative I considered** and **what I'd pick at 10× scale**.

| Choice | Picked | Why this, not the alternative | At 10× scale |
| ------ | ------ | ----------------------------- | ------------ |
| Backend framework | **Axum + Tokio** | Actix-Web is faster on benchmarks but its actor model is a poor fit when the only shared state is a snapshot map and a broadcast bus — Axum's tower-based extractor model keeps handlers as plain async fns and makes integration tests trivial. | Same shape, but the broadcast bus moves to NATS or Redis Streams once we exceed one backend instance. |
| Wire format (sim → backend) | **Protobuf (prost)** | JSON would have been fine for 5 trucks at 2 Hz, but protobuf matches Synapsis's production stack and forces an explicit schema with reserved field numbers (see `proto/fleet.proto`) — adding a field in 6 months won't break older clients. | Same — protobuf scales to thousands of trucks without re-litigation. |
| Push transport (backend → web) | **SSE** | WebSockets are bidirectional and we don't need that. SSE is one-way HTTP, auto-reconnects in the browser with no library code, plays nicely with proxies, and degrades to a `curl -N` for debugging. The cost (no client→server messages on the same channel) is irrelevant here. | Same up to ~10k concurrent dashboards per node. Beyond that, push goes through a fan-out service (NATS JetStream) and SSE remains the last mile. |
| Simulator language | **Rust** | Sharing the proto crate across simulator and backend means one regenerated artifact instead of two. Could have been Python in 30 fewer lines — but each truck is its own tokio task and the sustained-window state machine is easier to reason about with proper enums. | Same — the simulator becomes a load generator and Rust's per-task cost stays flat. |
| Frontend framework | **React + TS** | Solid/Svelte would re-render less by default, but the "single GeoJSON source, no per-marker components" pattern (see `FleetMap.tsx`) eliminates the re-render problem at its source — and React + TS is what Synapsis ships, so this aligns with the existing team's debugging muscle. | Same. The map data flow is the bottleneck, not the UI framework. |
| Map library | **MapLibre GL** | CesiumJS is the production stack and would have scored stretch points, but a half-finished 3D view is a hard fail per the brief. MapLibre is the same vector-tile model, free of API keys, and good enough for an open-pit top-down view. | CesiumJS for 3D pit visualisation; MapLibre stays for the 2D summary view. |
| Client state | **Zustand** | Redux Toolkit would force a boilerplate tax we don't earn back at this size. Zustand exposes a pure reducer (`reducer.ts`) for tests and a hook for components — that's the entire surface area. | Same up to a handful of stores. Beyond that, Redux Toolkit's structure starts paying for itself. |
| History storage | **In-memory ring buffer (1200 samples ≈ 10 min @ 2 Hz)** | Persistent storage was explicitly out of scope; a `Mutex<VecDeque>` per truck is 30 lines and matches what a dispatcher actually wants to scrub through. SQLite would have added a migration story for zero benefit. | TimescaleDB or InfluxDB. The ring buffer becomes the hot tier, the TSDB the warm tier; the `/api/trucks/:id/history` endpoint stays unchanged. |
| Bring-up | **docker compose** | Reviewers run on their own laptop. A single command beats a README that lists 4 prerequisites. | Compose for local dev, Helm + ArgoCD for production — same Dockerfiles. |

## Edge cases considered

The brief explicitly says the rubric scores domain thinking, not just
happy-path correctness. Each of the following is implemented and why
it matters to a dispatcher is documented in code:

- **GPS dropout shown as Stale, not Fault.** A truck behind a high wall
  isn't broken; treating dropout as a fault would generate noise the
  dispatcher learns to ignore. `health.rs::gps_stale` emits with severity
  `Info`.
- **Sensor-noise rejection.** A single-tick RPM spike is sensor noise,
  not over-rev. `health.rs::sustained` requires the predicate to hold
  across a contiguous run of samples spanning ≥ 5s before firing.
- **Truck stuck between states.** No telemetry for >2 min triggers a
  `Stuck` event — the simulator may have crashed, the radio may be
  down, or the operator may have left the cab without powering off.
  All of those are dispatch-relevant.
- **Load disagrees with state.** `Loaded` reported during `Returning`,
  or `Empty` during `Hauling`, fires `LoadMismatch`. This is exactly
  the kind of "the system says one thing but reality says another"
  signal a dispatcher needs to see early.
- **Out-of-order frames on the wire.** The web reducer drops telemetry
  whose `ts_unix_ms` is older than the latest seen for that truck.
  Snapshots are always accepted (authoritative).
- **Slow SSE clients.** `tokio::broadcast` returns `Lagged(n)` for slow
  subscribers; the stream handler converts that to a `resync` event so
  the affected client refetches the snapshot — without backpressuring
  the fast clients or the ingest path.
- **Malformed ingest body.** Body size cap (64 KiB), prost decode errors
  return 400 with a short reason and log at `warn`, not `error` — a
  misbehaving producer can't spam the error log.
- **Stationary truck in history view.** Collapsed bbox is guarded so the
  trail SVG doesn't divide by zero; no GPS in window renders an explicit
  empty state instead of a blank panel.

## Tests

Beyond the happy path, three meaningful cases per the brief:

- **Concurrency case** — `crates/fleet-backend/tests/concurrency.rs`:
  50 SSE subscribers + 10 Hz ingest, with one slow client; assert all
  fast clients see every health event, slow client gets `resync`.
- **Failure path** — `crates/fleet-backend/src/handlers/ingest.rs::tests`:
  malformed body → 400, oversized body → 400, missing `truck_id` → 400,
  state unchanged in every case.
- **Boundary** — `crates/fleet-backend/src/state.rs` ring buffer test:
  push N+1, length stays at N, oldest sample is gone.
- **Reducer purity** — `web/src/reducer.test.ts`: out-of-order frames,
  duplicate frames, health event for unknown truck — all idempotent.
- **Health rules** — `crates/fleet-backend/src/health.rs::tests`: each
  rule has a positive and a near-miss negative test (single-spike
  doesn't fire over-rev, normal drain doesn't fire fuel anomaly, etc.).

## AI usage log

**What was delegated.** Claude (via Claude Code) generated:
- the prost-build wiring in `fleet-proto/build.rs`,
- the SSE handler skeleton (`Sse::new(stream).keep_alive(...)`),
- the Zustand store wrapping the pure reducer,
- the docker compose multi-stage Dockerfiles with cargo-chef caching,
- the MapLibre style boilerplate (raster OSM source + circle layer).

**What I wrote or heavily rewrote.** The truck state machine and dwell
distributions, the `tokio::broadcast` ↔ `Lagged` → `resync` translation,
the sustained-window health classifier (every threshold and the
near-miss tests), the per-truck ring-buffer aggregation, and the
"single GeoJSON source, no per-marker components" map pattern were
all hand-written or rewritten from a first AI draft that didn't reason
about backpressure or React re-render cost.

**One concrete case where the AI was wrong.** While building the
history view I asked Claude to add a "ghost marker" layer on the
MapLibre map for the scrubber's selected timestamp. The generated
paint block included `'circle-stroke-dasharray': [2, 2]` — looks
plausible because line layers do support `line-dasharray`, but circle
layers don't have that property in the MapLibre spec. The map would
have either silently ignored it or thrown a style-validation error
on load, depending on version. I caught it on review (the marker
needed to read as "not a real truck", and dashed-stroke was the
specific affordance I'd asked for; when I went to confirm the
property name in the MapLibre style spec I found it didn't exist).
The fix was to drop the property and use lower opacity + a different
fill instead — the affordance the dispatcher actually needs is
"distinguishable from the real markers", not "dashed specifically",
so the AI's confidence on a non-existent property would have been
the kind of plausible-but-wrong output that makes uncritical AI use
dangerous.

## What's next

Three things consciously left out, with rationale:

1. **Persistent history (SQLite or TimescaleDB).** The 10-minute
   in-memory window is what a dispatcher needs *now*; longer-tail
   analysis is a separate use case (root-cause for an incident) with
   different access patterns and retention policy. Building both into
   one ring buffer would have produced a worse version of each. At
   10× scale this becomes a TimescaleDB warm tier behind the same
   `/api/trucks/:id/history` endpoint.
2. **Auth / multi-tenant.** Single-operator dashboard on a trusted
   network — adding JWT plumbing and an org model would have been
   ceremony with no demonstrable benefit at this scope, and the wrong
   shape if the real system ends up using mTLS at the edge plus an
   identity-aware proxy.
3. **3D / CesiumJS.** Stretch points were on offer, but the brief
   explicitly says "half-finished stretch items count against you"
   and a credible 3D pit view (terrain tiles, camera controls,
   coordinate system) is a multi-day task on its own. MapLibre
   covers the dispatcher's actual question ("where is each truck
   right now?") without the risk.

## Repository layout

```
proto/                 # .proto schema (single source of truth)
crates/
  fleet-proto/         # generated Rust types (prost-build)
  fleet-sim/           # 5-truck simulator
  fleet-backend/       # axum service
web/                   # React + TS dashboard
docker-compose.yml
TASKS.md               # detailed day-by-day breakdown
TODO.md                # higher-level plan
```
