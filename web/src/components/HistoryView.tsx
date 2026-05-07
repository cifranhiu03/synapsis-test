// Per-truck history panel: fetches a recent slice of telemetry from the
// backend ring buffer and renders three coordinated views — a mini SVG
// trail, a speed sparkline, and a state-band strip — all driven by a
// single scrubber. The scrubber publishes a "ghost" position to the
// store; the main map renders it as a dashed marker so the dispatcher
// can correlate past position with current fleet context.

import { useEffect, useMemo, useState } from 'react'
import { useFleetStore } from '../store'
import type { Truck, TruckStateLabel } from '../types'

const HISTORY_WINDOW_MS = 10 * 60 * 1000

const STATE_COLOR: Record<TruckStateLabel, string> = {
  IDLE: '#9ca3af',
  LOADING_QUEUE: '#fbbf24',
  LOADING: '#f59e0b',
  HAULING: '#3b82f6',
  AT_CRUSHER: '#8b5cf6',
  DUMPING: '#a855f7',
  RETURNING: '#10b981',
  UNKNOWN: '#ef4444',
}

interface Props { truckId: string }

export function HistoryView({ truckId }: Props) {
  const close = useFleetStore((s) => () => s.setHistoryOpen(false))
  const setGhost = useFleetStore((s) => s.setGhost)

  const [samples, setSamples] = useState<Truck[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [idx, setIdx] = useState(0)

  useEffect(() => {
    let cancelled = false
    const since = Date.now() - HISTORY_WINDOW_MS
    fetch(`/api/trucks/${encodeURIComponent(truckId)}/history?since_ms=${since}`)
      .then(async (res) => {
        if (!res.ok) throw new Error(`history ${res.status}`)
        const rows: Truck[] = await res.json()
        if (cancelled) return
        setSamples(rows)
        setIdx(Math.max(0, rows.length - 1))
      })
      .catch((e) => !cancelled && setError(String(e)))
    return () => { cancelled = true }
  }, [truckId])

  // Publish ghost on scrubber move; clear when this view unmounts.
  useEffect(() => {
    const s = samples?.[idx]
    if (s?.gps) setGhost({ lon: s.gps.lon, lat: s.gps.lat })
    else setGhost(null)
  }, [samples, idx, setGhost])
  useEffect(() => () => setGhost(null), [setGhost])

  const geom = useMemo(() => {
    if (!samples || samples.length === 0) return null
    const fixes = samples.filter((s) => s.gps)
    if (fixes.length === 0) return null
    const lons = fixes.map((s) => s.gps!.lon)
    const lats = fixes.map((s) => s.gps!.lat)
    const minLon = Math.min(...lons), maxLon = Math.max(...lons)
    const minLat = Math.min(...lats), maxLat = Math.max(...lats)
    const W = 280, H = 140, pad = 8
    // Guard against degenerate bbox (truck stationary).
    const dLon = maxLon - minLon || 1e-6
    const dLat = maxLat - minLat || 1e-6
    const project = (lon: number, lat: number) => {
      const x = pad + ((lon - minLon) / dLon) * (W - 2 * pad)
      const y = H - pad - ((lat - minLat) / dLat) * (H - 2 * pad)
      return [x, y] as const
    }
    const path = fixes
      .map((s, i) => {
        const [x, y] = project(s.gps!.lon, s.gps!.lat)
        return `${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`
      })
      .join(' ')
    return { W, H, path, project }
  }, [samples])

  if (error) {
    return (
      <div className="panel history">
        <header><h2>History</h2><button onClick={close}>×</button></header>
        <p className="empty">Failed to load: {error}</p>
      </div>
    )
  }
  if (!samples) {
    return (
      <div className="panel history">
        <header><h2>History · {truckId}</h2><button onClick={close}>×</button></header>
        <p className="empty">Loading…</p>
      </div>
    )
  }
  if (samples.length === 0) {
    return (
      <div className="panel history">
        <header><h2>History · {truckId}</h2><button onClick={close}>×</button></header>
        <p className="empty">No samples in the last 10 minutes.</p>
      </div>
    )
  }

  const cur = samples[idx]
  const t0 = samples[0].ts_unix_ms
  const tN = samples[samples.length - 1].ts_unix_ms
  const span = Math.max(1, tN - t0)
  const maxSpeed = Math.max(1, ...samples.map((s) => s.speed_kmh))

  // Speed sparkline (SVG path, 280×40).
  const SW = 280, SH = 40
  const speedPath = samples
    .map((s, i) => {
      const x = (i / Math.max(1, samples.length - 1)) * SW
      const y = SH - (s.speed_kmh / maxSpeed) * (SH - 4) - 2
      return `${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`
    })
    .join(' ')

  // State band: rectangles per contiguous-state run.
  const bandSegments: { x: number; w: number; state: TruckStateLabel }[] = []
  let runStart = 0
  for (let i = 1; i <= samples.length; i++) {
    if (i === samples.length || samples[i].state !== samples[runStart].state) {
      const x0 = ((samples[runStart].ts_unix_ms - t0) / span) * SW
      const xN = (((samples[i - 1].ts_unix_ms - t0) / span) * SW) || x0 + 1
      bandSegments.push({ x: x0, w: Math.max(1, xN - x0), state: samples[runStart].state })
      runStart = i
    }
  }

  const cursorX = ((cur.ts_unix_ms - t0) / span) * SW

  return (
    <div className="panel history">
      <header>
        <h2>History · {truckId}</h2>
        <button onClick={close} aria-label="Close">×</button>
      </header>

      {geom && (
        <svg className="history-trail" viewBox={`0 0 ${geom.W} ${geom.H}`} width="100%">
          <rect x="0" y="0" width={geom.W} height={geom.H} fill="#f8fafc" rx="6" />
          <path d={geom.path} stroke="#3b82f6" strokeWidth="2" fill="none" />
          {cur.gps && (() => {
            const [x, y] = geom.project(cur.gps.lon, cur.gps.lat)
            return <circle cx={x} cy={y} r="5" fill="#0f172a" />
          })()}
        </svg>
      )}

      <h3>Speed (km/h)</h3>
      <svg viewBox={`0 0 ${SW} ${SH}`} width="100%" height={SH}>
        <rect x="0" y="0" width={SW} height={SH} fill="#f8fafc" rx="4" />
        <path d={speedPath} stroke="#0f172a" strokeWidth="1.5" fill="none" />
        <line x1={cursorX} x2={cursorX} y1="0" y2={SH} stroke="#ef4444" strokeWidth="1" />
      </svg>

      <h3>State</h3>
      <svg viewBox={`0 0 ${SW} 16`} width="100%" height={16}>
        {bandSegments.map((seg, i) => (
          <rect key={i} x={seg.x} y="0" width={seg.w} height="16" fill={STATE_COLOR[seg.state]} />
        ))}
        <line x1={cursorX} x2={cursorX} y1="0" y2="16" stroke="#0f172a" strokeWidth="1" />
      </svg>

      <input
        className="scrubber"
        type="range"
        min={0}
        max={samples.length - 1}
        value={idx}
        onChange={(e) => setIdx(Number(e.target.value))}
      />
      <dl className="history-stats">
        <dt>At</dt>
        <dd>{new Date(cur.ts_unix_ms).toLocaleTimeString()}</dd>
        <dt>State</dt>
        <dd>{cur.state}</dd>
        <dt>Speed</dt>
        <dd>{cur.speed_kmh.toFixed(1)} km/h</dd>
        <dt>RPM</dt>
        <dd>{cur.rpm}</dd>
        <dt>Fuel</dt>
        <dd>{Math.round(cur.fuel_pct * 100)}%</dd>
      </dl>
    </div>
  )
}
