import { useFleetStore } from '../store'
import { summarize } from '../reducer'

const STATE_ORDER = [
  'IDLE',
  'LOADING_QUEUE',
  'LOADING',
  'HAULING',
  'AT_CRUSHER',
  'DUMPING',
  'RETURNING',
] as const

export function FleetSummary() {
  const trucks = useFleetStore((s) => s.trucks)
  const alerts = useFleetStore((s) => s.alerts)
  const connected = useFleetStore((s) => s.connected)
  const counts = summarize(trucks)
  const total = Object.values(trucks).length
  const activeFaults = alerts.filter((a) => a.severity === 'FAULT').length

  return (
    <div className="panel summary">
      <header>
        <h2>Fleet</h2>
        <span className={`pill ${connected ? 'ok' : 'bad'}`}>
          {connected ? 'live' : 'reconnecting…'}
        </span>
      </header>
      <div className="grid">
        <div className="big">
          <div className="num">{total}</div>
          <div className="lbl">trucks</div>
        </div>
        <div className="big">
          <div className="num">{activeFaults}</div>
          <div className="lbl">faults</div>
        </div>
      </div>
      <ul className="states">
        {STATE_ORDER.map((s) => (
          <li key={s}>
            <span className={`dot s-${s}`} />
            <span className="name">{s.replace('_', ' ')}</span>
            <span className="count">{counts[s] ?? 0}</span>
          </li>
        ))}
      </ul>
      <h3>Alerts</h3>
      <ul className="alerts">
        {alerts.length === 0 && <li className="empty">No alerts</li>}
        {alerts.slice(0, 10).map((a, i) => (
          <li key={`${a.truck_id}-${a.ts_unix_ms}-${i}`} className={`sev-${a.severity}`}>
            <span className="who">{a.truck_id}</span>
            <span className="kind">{a.kind}</span>
            <span className="msg">{a.message}</span>
          </li>
        ))}
      </ul>
    </div>
  )
}
