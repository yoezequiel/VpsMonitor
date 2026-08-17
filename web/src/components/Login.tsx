import { useState } from 'react'
import { login } from '../api'

export function Login({ onSuccess }: { onSuccess: () => void }) {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setError(null)
    setBusy(true)
    try {
      await login(username, password)
      onSuccess()
    } catch {
      setError('Invalid username or password')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div style={{ maxWidth: 340, margin: '15vh auto 0' }}>
      <h1 style={{ fontSize: 20, marginBottom: 24 }}>Self-Hosted Infrastructure Monitor</h1>
      <form onSubmit={handleSubmit} className="card" style={{ display: 'grid', gap: 12 }}>
        <label style={{ display: 'grid', gap: 4 }}>
          <span className="secondary" style={{ fontSize: 13 }}>
            Username
          </span>
          <input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoFocus
            required
            style={inputStyle}
          />
        </label>
        <label style={{ display: 'grid', gap: 4 }}>
          <span className="secondary" style={{ fontSize: 13 }}>
            Password
          </span>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
            style={inputStyle}
          />
        </label>
        {error && (
          <div style={{ color: 'var(--status-critical)', fontSize: 13 }}>{error}</div>
        )}
        <button type="submit" disabled={busy} style={buttonStyle}>
          {busy ? 'Signing in…' : 'Sign in'}
        </button>
      </form>
    </div>
  )
}

const inputStyle: React.CSSProperties = {
  padding: '8px 10px',
  borderRadius: 6,
  border: '1px solid var(--border)',
  background: 'var(--page)',
  color: 'var(--text-primary)',
}

const buttonStyle: React.CSSProperties = {
  padding: '8px 10px',
  borderRadius: 6,
  border: 'none',
  background: 'var(--series-blue)',
  color: '#fff',
  fontWeight: 600,
  cursor: 'pointer',
}
