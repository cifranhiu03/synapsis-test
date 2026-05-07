import { useEffect } from 'react'
import { useFleetStore } from './store'
import type { StreamEvent, Truck } from './types'

// EventSource is auto-reconnecting. We additionally hydrate from
// `/api/fleet` on mount so the map paints before the first SSE frame
// (which can take up to one tick), and again after a reconnect so we
// don't leak stale state across a dropped connection.

export function useStream() {
  const apply = useFleetStore((s) => s.apply)
  const setConnected = useFleetStore((s) => s.setConnected)
  const hydrate = useFleetStore((s) => s.hydrate)

  useEffect(() => {
    let cancelled = false

    const refetch = async () => {
      try {
        const res = await fetch('/api/fleet')
        if (!res.ok) return
        const trucks: Truck[] = await res.json()
        if (!cancelled) hydrate(trucks)
      } catch {
        /* network blip — SSE will retry */
      }
    }

    refetch()
    const es = new EventSource('/api/stream')

    es.onopen = () => {
      setConnected(true)
      refetch()
    }
    es.onerror = () => setConnected(false)
    es.onmessage = (ev) => {
      try {
        const parsed = JSON.parse(ev.data) as StreamEvent
        apply(parsed)
        if (parsed.kind === 'resync') refetch()
      } catch {
        // Malformed frame — drop, don't crash the dashboard.
      }
    }

    return () => {
      cancelled = true
      es.close()
    }
  }, [apply, setConnected, hydrate])
}
