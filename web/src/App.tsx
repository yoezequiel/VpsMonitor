import { useState } from 'react'
import { getToken } from './api'
import { Login } from './components/Login'
import { ServerList } from './components/ServerList'
import { ServerDetail } from './components/ServerDetail'

export default function App() {
  const [authed, setAuthed] = useState(() => getToken() !== null)
  const [selectedId, setSelectedId] = useState<string | null>(null)

  if (!authed) {
    return <Login onSuccess={() => setAuthed(true)} />
  }

  if (selectedId) {
    return <ServerDetail id={selectedId} onBack={() => setSelectedId(null)} />
  }

  return (
    <ServerList
      onSelect={setSelectedId}
      onLoggedOut={() => {
        setAuthed(false)
        setSelectedId(null)
      }}
    />
  )
}
