import { useState } from 'react'
import { getToken, logout } from './api'
import { Login } from './components/Login'
import { ServerList } from './components/ServerList'
import { ServerDetail } from './components/ServerDetail'
import { AlertsView } from './components/AlertsView'
import { IncidentsView } from './components/IncidentsView'
import { HealthChecksView } from './components/HealthChecksView'
import { GroupsView } from './components/GroupsView'

type Tab = 'infrastructure' | 'alerts' | 'incidents' | 'healthchecks' | 'groups'

const TABS: { id: Tab; label: string }[] = [
  { id: 'infrastructure', label: 'Infrastructure' },
  { id: 'alerts', label: 'Alerts' },
  { id: 'incidents', label: 'Incidents' },
  { id: 'healthchecks', label: 'Health Checks' },
  { id: 'groups', label: 'Groups' },
]

export default function App() {
  const [authed, setAuthed] = useState(() => getToken() !== null)
  const [tab, setTab] = useState<Tab>('infrastructure')
  const [selectedId, setSelectedId] = useState<string | null>(null)

  if (!authed) {
    return <Login onSuccess={() => setAuthed(true)} />
  }

  function handleLoggedOut() {
    setAuthed(false)
    setSelectedId(null)
  }

  if (selectedId) {
    return <ServerDetail id={selectedId} onBack={() => setSelectedId(null)} />
  }

  return (
    <div>
      <nav
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: 20,
          borderBottom: '1px solid var(--border)',
          paddingBottom: 10,
        }}
      >
        <div style={{ display: 'flex', gap: 4 }}>
          {TABS.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              style={{
                padding: '6px 12px',
                borderRadius: 6,
                border: 'none',
                background: tab === t.id ? 'var(--series-blue)' : 'none',
                color: tab === t.id ? '#fff' : 'var(--text-secondary)',
                cursor: 'pointer',
                fontSize: 13,
                fontWeight: tab === t.id ? 600 : 400,
              }}
            >
              {t.label}
            </button>
          ))}
        </div>
        <button
          onClick={async () => {
            await logout()
            handleLoggedOut()
          }}
          style={{ background: 'none', border: 'none', color: 'var(--text-secondary)', cursor: 'pointer', fontSize: 13 }}
        >
          Sign out
        </button>
      </nav>

      {tab === 'infrastructure' && <ServerList onSelect={setSelectedId} onLoggedOut={handleLoggedOut} embedded />}
      {tab === 'alerts' && <AlertsView />}
      {tab === 'incidents' && <IncidentsView />}
      {tab === 'healthchecks' && <HealthChecksView />}
      {tab === 'groups' && <GroupsView />}
    </div>
  )
}
