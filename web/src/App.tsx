import { FleetMap } from './components/FleetMap'
import { FleetSummary } from './components/FleetSummary'
import { TruckDetail } from './components/TruckDetail'
import { useStream } from './useStream'
import './App.css'

function App() {
  useStream()
  return (
    <div className="layout">
      <aside className="sidebar">
        <FleetSummary />
      </aside>
      <main className="map-wrap">
        <FleetMap />
        <TruckDetail />
      </main>
    </div>
  )
}

export default App
