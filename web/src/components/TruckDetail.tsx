import { useFleetStore } from '../store'

export function TruckDetail() {
  const id = useFleetStore((s) => s.selectedTruckId)
  const truck = useFleetStore((s) => (id ? s.trucks[id] : undefined))
  const close = useFleetStore((s) => () => s.selectTruck(null))

  if (!id) return null
  if (!truck) {
    return (
      <div className="panel detail">
        <header>
          <h2>{id}</h2>
          <button onClick={close} aria-label="Close">×</button>
        </header>
        <p className="empty">No telemetry yet.</p>
      </div>
    )
  }

  const fuelPct = Math.round(truck.fuel_pct * 100)
  const stale = (truck.age_ms ?? 0) > 10_000

  return (
    <div className="panel detail">
      <header>
        <h2>{truck.truck_id}</h2>
        <button onClick={close} aria-label="Close">×</button>
      </header>
      <dl>
        <dt>State</dt>
        <dd>{truck.state}{stale ? ' · stale' : ''}</dd>
        <dt>Speed</dt>
        <dd>{truck.speed_kmh.toFixed(1)} km/h</dd>
        <dt>RPM</dt>
        <dd>{truck.rpm}</dd>
        <dt>Load</dt>
        <dd>{truck.load}</dd>
        <dt>Fuel</dt>
        <dd>{fuelPct}%</dd>
        <dt>Position</dt>
        <dd>
          {truck.gps
            ? `${truck.gps.lat.toFixed(5)}, ${truck.gps.lon.toFixed(5)}`
            : 'no fix'}
        </dd>
        <dt>Updated</dt>
        <dd>
          {new Date(truck.ts_unix_ms).toLocaleTimeString()}
          {truck.age_ms !== undefined ? ` · ${Math.round(truck.age_ms / 100) / 10}s ago` : ''}
        </dd>
      </dl>
    </div>
  )
}
