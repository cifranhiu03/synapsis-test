import { create } from 'zustand'
import { reduce, initialState, type FleetState } from './reducer'
import type { StreamEvent, Truck } from './types'

interface UiState {
  selectedTruckId: string | null
  /** When the history scrubber is moved, points to a past GPS fix
   * for the selected truck so the main map can render a ghost marker. */
  ghost: { lon: number; lat: number } | null
  /** True while the history panel is mounted. */
  historyOpen: boolean
}

interface Actions {
  apply: (event: StreamEvent) => void
  setConnected: (v: boolean) => void
  selectTruck: (id: string | null) => void
  hydrate: (trucks: Truck[]) => void
  setGhost: (g: { lon: number; lat: number } | null) => void
  setHistoryOpen: (v: boolean) => void
}

export const useFleetStore = create<FleetState & UiState & Actions>((set) => ({
  ...initialState,
  selectedTruckId: null,
  ghost: null,
  historyOpen: false,
  apply: (event) => set((s) => reduce(s, event)),
  setConnected: (connected) => set({ connected }),
  selectTruck: (id) =>
    set({ selectedTruckId: id, historyOpen: false, ghost: null }),
  hydrate: (trucks) =>
    set((s) => reduce(s, { kind: 'snapshot', data: trucks })),
  setGhost: (ghost) => set({ ghost }),
  setHistoryOpen: (historyOpen) =>
    set((s) => ({ historyOpen, ghost: historyOpen ? s.ghost : null })),
}))
