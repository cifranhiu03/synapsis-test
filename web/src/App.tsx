import { FleetMap } from './components/FleetMap'
import { FleetSummary } from './components/FleetSummary'
import { TruckDetail } from './components/TruckDetail'
import { HistoryView } from './components/HistoryView'
import { useFleetStore } from './store'
import { useStream } from './useStream'
import './App.css'

function App() {
  useStream()
  const selectedId = useFleetStore((s) => s.selectedTruckId)
  const historyOpen = useFleetStore((s) => s.historyOpen)
  return (
    <div className="layout">
      <aside className="sidebar">
        <FleetSummary />
      </aside>
      <main className="map-wrap">
        <FleetMap />
        <TruckDetail />
        {selectedId && historyOpen && <HistoryView truckId={selectedId} />}
      </main>
    </div>
  )
}

export default App
