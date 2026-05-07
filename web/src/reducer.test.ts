import { describe, it, expect } from 'vitest'
import { reduce, initialState, summarize } from './reducer'
import type { Truck } from './types'

const truck = (id: string, ts: number, overrides: Partial<Truck> = {}): Truck => ({
  truck_id: id,
  ts_unix_ms: ts,
  speed_kmh: 0,
  rpm: 800,
  load: 'EMPTY',
  fuel_pct: 0.9,
  state: 'IDLE',
  ...overrides,
})

describe('reduce', () => {
  it('hydrates from a snapshot', () => {
    const next = reduce(initialState, {
      kind: 'snapshot',
      data: [truck('T1', 100), truck('T2', 100)],
    })
    expect(Object.keys(next.trucks)).toEqual(['T1', 'T2'])
  })

  it('replaces fleet on a later snapshot (no leak of removed trucks)', () => {
    const a = reduce(initialState, {
      kind: 'snapshot',
      data: [truck('T1', 100), truck('T2', 100)],
    })
    const b = reduce(a, { kind: 'snapshot', data: [truck('T1', 200)] })
    expect(Object.keys(b.trucks)).toEqual(['T1'])
  })

  it('drops out-of-order telemetry', () => {
    const a = reduce(initialState, { kind: 'telemetry', data: truck('T1', 200) })
    const b = reduce(a, { kind: 'telemetry', data: truck('T1', 150, { speed_kmh: 99 }) })
    expect(b.trucks.T1.speed_kmh).toBe(0)
    expect(b.trucks.T1.ts_unix_ms).toBe(200)
  })

  it('ignores duplicate telemetry at the same timestamp', () => {
    const a = reduce(initialState, { kind: 'telemetry', data: truck('T1', 200, { speed_kmh: 10 }) })
    const b = reduce(a, { kind: 'telemetry', data: truck('T1', 200, { speed_kmh: 99 }) })
    expect(b.trucks.T1.speed_kmh).toBe(10)
  })

  it('caps alerts at 50 and prepends newest', () => {
    let s = initialState
    for (let i = 0; i < 60; i++) {
      s = reduce(s, {
        kind: 'health',
        data: { truck_id: 'T1', ts_unix_ms: i, kind: 'OVER_REV', severity: 'WARN', message: `n${i}` },
      })
    }
    expect(s.alerts).toHaveLength(50)
    expect(s.alerts[0].message).toBe('n59')
  })

  it('resync is a no-op on truck state', () => {
    const a = reduce(initialState, { kind: 'snapshot', data: [truck('T1', 100)] })
    const b = reduce(a, { kind: 'resync' })
    expect(b.trucks).toBe(a.trucks)
  })
})

describe('summarize', () => {
  it('counts trucks by state', () => {
    const counts = summarize({
      a: truck('a', 1, { state: 'IDLE' }),
      b: truck('b', 1, { state: 'HAULING' }),
      c: truck('c', 1, { state: 'HAULING' }),
    })
    expect(counts).toEqual({ IDLE: 1, HAULING: 2 })
  })
})
