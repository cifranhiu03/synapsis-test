# Mine Fleet Live Tracker

Real-time dashboard for an open-pit mine dispatcher: 5 simulated haul trucks,
live telemetry, health alerts, per-vehicle history. Built for the Synapsis
Sr. Full-Stack Software Engineer take-home.

> Status: scaffolding (Day 1). Sections below will be filled in across the
> 7-day plan in `TASKS.md`.

## Architecture

_TODO: one diagram, one paragraph._

```
[fleet-sim] --proto/HTTP--> [fleet-backend (axum)] --SSE/JSON--> [web (React + MapLibre)]
```

## How to run it

Prerequisites: Docker ≥ 24, ports 5173 and 8080 free.

```sh
docker compose up --build
```

Open <http://localhost:5173>.

## Stack decisions

| Choice | Picked | Why it beats the alternatives | At 10× scale |
| ------ | ------ | ----------------------------- | ------------ |
| Backend framework | Axum + Tokio | _TODO_ | _TODO_ |
| Wire format | Protobuf (prost) | _TODO_ | _TODO_ |
| Push transport | SSE | _TODO_ | _TODO_ |
| Simulator language | Rust | _TODO_ | _TODO_ |
| Frontend | React + TS + MapLibre | _TODO_ | _TODO_ |
| State store | Zustand | _TODO_ | _TODO_ |
| History storage | In-memory ring buffer | _TODO_ | _TODO_ |
| Bring-up | docker compose | _TODO_ | _TODO_ |

## Edge cases considered

_TODO (Day 6)._

## Tests

_TODO._

## AI usage log

_TODO. Will include: what was delegated, what was hand-written, and one
concrete case where the AI was wrong._

## What's next

_TODO. Two or three things deliberately not shipped, and why._
