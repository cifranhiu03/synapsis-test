// Pure reducer for the dashboard's view of the fleet. Kept free of React
// or zustand so it's trivially testable: feed it the previous state and a
// stream event, get the next state.
//
// Out-of-order frames are common at 2 Hz × 5 trucks across a flaky link:
// we drop telemetry whose `ts_unix_ms` is older than what we already have
// for that truck, but always accept snapshots (they're authoritative).

import type { HealthEvent, StreamEvent, Truck } from './types'

const MAX_ALERTS = 50

export interface FleetState {
  trucks: Record<string, Truck>
  alerts: HealthEvent[]
  /** Server-assigned id from the SSE stream; latest seen. */
  lastEventId?: string
  connected: boolean
}

export const initialState: FleetState = {
  trucks: {},
  alerts: [],
  connected: false,
}

export function reduce(state: FleetState, event: StreamEvent): FleetState {
  switch (event.kind) {
    case 'snapshot': {
      const trucks: Record<string, Truck> = {}
      for (const t of event.data) trucks[t.truck_id] = t
      return { ...state, trucks }
    }
    case 'telemetry': {
      const t = event.data
      const prev = state.trucks[t.truck_id]
      if (prev && prev.ts_unix_ms >= t.ts_unix_ms) return state
      return { ...state, trucks: { ...state.trucks, [t.truck_id]: t } }
    }
    case 'health': {
      const alerts = [event.data, ...state.alerts].slice(0, MAX_ALERTS)
      return { ...state, alerts }
    }
    case 'resync': {
      // Backend told us to refetch; we keep current trucks visible while
      // the snapshot is in flight rather than blanking the map.
      return state
    }
  }
}

export function summarize(trucks: Record<string, Truck>) {
  const counts: Record<string, number> = {}
  for (const t of Object.values(trucks)) {
    counts[t.state] = (counts[t.state] ?? 0) + 1
  }
  return counts
}
