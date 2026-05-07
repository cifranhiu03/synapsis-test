import { create } from 'zustand'
import { reduce, initialState, type FleetState } from './reducer'
import type { StreamEvent, Truck } from './types'

interface UiState {
  selectedTruckId: string | null
}

interface Actions {
  apply: (event: StreamEvent) => void
  setConnected: (v: boolean) => void
  selectTruck: (id: string | null) => void
  hydrate: (trucks: Truck[]) => void
}

export const useFleetStore = create<FleetState & UiState & Actions>((set) => ({
  ...initialState,
  selectedTruckId: null,
  apply: (event) => set((s) => reduce(s, event)),
  setConnected: (connected) => set({ connected }),
  selectTruck: (id) => set({ selectedTruckId: id }),
  hydrate: (trucks) =>
    set((s) => reduce(s, { kind: 'snapshot', data: trucks })),
}))
