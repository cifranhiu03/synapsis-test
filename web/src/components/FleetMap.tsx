import { useEffect, useRef } from 'react'
import maplibregl, { type Map as MlMap } from 'maplibre-gl'
import 'maplibre-gl/dist/maplibre-gl.css'
import { useFleetStore } from '../store'
import type { Truck } from '../types'

// One GeoJSON source for all trucks (not per-marker components): MapLibre
// re-renders the source on each setData call without remounting markers,
// which keeps update cost flat as the fleet grows.

const PIT_CENTER: [number, number] = [117.5620, 0.5380]

const STATE_COLOR: Record<string, string> = {
  IDLE: '#9ca3af',
  LOADING_QUEUE: '#fbbf24',
  LOADING: '#f59e0b',
  HAULING: '#3b82f6',
  AT_CRUSHER: '#8b5cf6',
  DUMPING: '#a855f7',
  RETURNING: '#10b981',
  UNKNOWN: '#ef4444',
}

function toFeatureCollection(trucks: Truck[]) {
  return {
    type: 'FeatureCollection' as const,
    features: trucks
      .filter((t) => t.gps)
      .map((t) => ({
        type: 'Feature' as const,
        geometry: {
          type: 'Point' as const,
          coordinates: [t.gps!.lon, t.gps!.lat],
        },
        properties: {
          truck_id: t.truck_id,
          state: t.state,
          color: STATE_COLOR[t.state] ?? STATE_COLOR.UNKNOWN,
          stale: (t.age_ms ?? 0) > 10_000,
        },
      })),
  }
}

export function FleetMap() {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const mapRef = useRef<MlMap | null>(null)
  const trucks = useFleetStore((s) => s.trucks)
  const select = useFleetStore((s) => s.selectTruck)
  const ghost = useFleetStore((s) => s.ghost)

  useEffect(() => {
    if (!containerRef.current || mapRef.current) return
    const map = new maplibregl.Map({
      container: containerRef.current,
      style: {
        version: 8,
        sources: {
          osm: {
            type: 'raster',
            tiles: ['https://tile.openstreetmap.org/{z}/{x}/{y}.png'],
            tileSize: 256,
            attribution: '© OpenStreetMap',
          },
        },
        layers: [{ id: 'osm', type: 'raster', source: 'osm' }],
      },
      center: PIT_CENTER,
      zoom: 14,
    })

    map.on('load', () => {
      map.addSource('trucks', {
        type: 'geojson',
        data: { type: 'FeatureCollection', features: [] },
      })
      map.addLayer({
        id: 'trucks-circle',
        type: 'circle',
        source: 'trucks',
        paint: {
          'circle-radius': 9,
          'circle-color': ['get', 'color'],
          'circle-stroke-color': '#0f172a',
          'circle-stroke-width': 2,
          'circle-opacity': ['case', ['get', 'stale'], 0.4, 1],
        },
      })
      map.addSource('ghost', {
        type: 'geojson',
        data: { type: 'FeatureCollection', features: [] },
      })
      map.addLayer({
        id: 'ghost-circle',
        type: 'circle',
        source: 'ghost',
        paint: {
          'circle-radius': 11,
          'circle-color': '#0f172a',
          'circle-opacity': 0,
          'circle-stroke-color': '#0f172a',
          'circle-stroke-width': 2,
        },
      })
      map.addLayer({
        id: 'trucks-label',
        type: 'symbol',
        source: 'trucks',
        layout: {
          'text-field': ['get', 'truck_id'],
          'text-size': 11,
          'text-offset': [0, 1.2],
        },
        paint: {
          'text-color': '#0f172a',
          'text-halo-color': '#fff',
          'text-halo-width': 1.2,
        },
      })

      map.on('click', 'trucks-circle', (e) => {
        const f = e.features?.[0]
        if (f) select(String(f.properties?.truck_id))
      })
      map.on('mouseenter', 'trucks-circle', () => {
        map.getCanvas().style.cursor = 'pointer'
      })
      map.on('mouseleave', 'trucks-circle', () => {
        map.getCanvas().style.cursor = ''
      })
    })

    mapRef.current = map
    return () => {
      map.remove()
      mapRef.current = null
    }
  }, [select])

  useEffect(() => {
    const map = mapRef.current
    if (!map) return
    const apply = () => {
      const src = map.getSource('trucks') as maplibregl.GeoJSONSource | undefined
      if (src) src.setData(toFeatureCollection(Object.values(trucks)))
    }
    if (map.isStyleLoaded()) apply()
    else map.once('load', apply)
  }, [trucks])

  useEffect(() => {
    const map = mapRef.current
    if (!map) return
    const apply = () => {
      const src = map.getSource('ghost') as maplibregl.GeoJSONSource | undefined
      if (!src) return
      src.setData({
        type: 'FeatureCollection',
        features: ghost
          ? [{
              type: 'Feature',
              geometry: { type: 'Point', coordinates: [ghost.lon, ghost.lat] },
              properties: {},
            }]
          : [],
      })
    }
    if (map.isStyleLoaded()) apply()
    else map.once('load', apply)
  }, [ghost])

  return <div ref={containerRef} className="map" />
}
