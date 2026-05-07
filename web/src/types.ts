// Mirror of the backend's `dto.rs`. Hand-written rather than generated:
// the JSON shape is small, and a 200-line codegen pipeline would dwarf
// the schema it produces.

export type TruckStateLabel =
  | 'IDLE'
  | 'LOADING_QUEUE'
  | 'LOADING'
  | 'HAULING'
  | 'AT_CRUSHER'
  | 'DUMPING'
  | 'RETURNING'
  | 'UNKNOWN'

export type LoadLabel = 'EMPTY' | 'LOADED' | 'UNKNOWN'

export type SeverityLabel = 'INFO' | 'WARN' | 'FAULT' | 'UNKNOWN'

export type HealthKindLabel =
  | 'OVER_REV'
  | 'UNSAFE_SPEED_UNDER_LOAD'
  | 'EXCESSIVE_IDLE'
  | 'FUEL_ANOMALY'
  | 'GPS_STALE'
  | 'STUCK'
  | 'LOAD_MISMATCH'
  | 'UNKNOWN'

export interface Gps {
  lat: number
  lon: number
  alt_m: number
  hdop: number
}

export interface Truck {
  truck_id: string
  ts_unix_ms: number
  gps?: Gps
  speed_kmh: number
  rpm: number
  load: LoadLabel
  fuel_pct: number
  state: TruckStateLabel
  age_ms?: number
}

export interface HealthEvent {
  truck_id: string
  ts_unix_ms: number
  kind: HealthKindLabel
  severity: SeverityLabel
  message: string
}

export type StreamEvent =
  | { kind: 'snapshot'; data: Truck[] }
  | { kind: 'telemetry'; data: Truck }
  | { kind: 'health'; data: HealthEvent }
  | { kind: 'resync'; data?: null }
