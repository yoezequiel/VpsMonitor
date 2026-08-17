import { useEffect, useState } from 'react'
import { connectStatusStream, listServers, logout } from '../api'
import type { Server, ServerStatus } from '../types'
import { StatusDot } from './StatusDot'

function relativeTime(iso: string | null): string {
  if (!iso) return 'never'
  const seconds = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000)
  if (seconds < 60) return `${Math.floor(seconds)}s ago`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`
  return `${Math.floor(seconds / 86400)}d ago`
}

function countByStatus(servers: Server[], status: ServerStatus): number {
  return servers.filter((s) => s.status === status).length
}

export function ServerList({
  onSelect,
  onLoggedOut,
}: {
  onSelect: (id: string) => void
  onLoggedOut: () => void
}) {
  const [servers, setServers] = useState<Server[]>([])
  const [error, setError] = useState<string | null>(null)

  async function refresh() {
    try {
      const data = await listServers()
      data.sort((a, b) => a.name.localeCompare(b.name))
      setServers(data)
    } catch {
      setError('Session expired')
      onLoggedOut()
    }
  }

  useEffect(() => {
    refresh()
    const interval = setInterval(refresh, 30_000)
    const disconnect = connectStatusStream((event) => {
      setServers((prev) =>
        prev.map((s) => (s.id === event.server_id ? { ...s, status: event.status } : s)),
      )
    })
    return () => {
      clearInterval(interval)
      disconnect()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  if (error) return null

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 16 }}>
        <h1 style={{ fontSize: 20, margin: 0 }}>Infrastructure</h1>
        <button
          onClick={async () => {
            await logout()
            onLoggedOut()
          }}
          style={{ background: 'none', border: 'none', color: 'var(--text-secondary)', cursor: 'pointer' }}
        >
          Sign out
        </button>
      </div>

      <div className="card" style={{ display: 'flex', gap: 24, marginBottom: 16 }}>
        <SummaryStat color="var(--status-good)" label="Online" value={countByStatus(servers, 'online')} />
        <SummaryStat color="var(--status-critical)" label="Offline" value={countByStatus(servers, 'offline')} />
        <SummaryStat color="var(--status-warning)" label="Warning" value={countByStatus(servers, 'warning')} />
      </div>

      <div className="card" style={{ padding: 0 }}>
        {servers.length === 0 ? (
          <div style={{ padding: 16 }} className="muted">
            No servers registered yet. Use <code>monitor server add --name &lt;name&gt;</code> to add one.
          </div>
        ) : (
          servers.map((server) => (
            <button
              key={server.id}
              onClick={() => onSelect(server.id)}
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                width: '100%',
                padding: '12px 16px',
                border: 'none',
                borderBottom: '1px solid var(--border)',
                background: 'none',
                color: 'inherit',
                textAlign: 'left',
                cursor: 'pointer',
              }}
            >
              <div>
                <div style={{ fontWeight: 600 }}>{server.name}</div>
                <div className="muted" style={{ fontSize: 13 }}>
                  {server.hostname} · {server.os}
                </div>
              </div>
              <div style={{ textAlign: 'right' }}>
                <StatusDot status={server.status} />
                <div className="muted" style={{ fontSize: 12 }}>
                  {relativeTime(server.last_heartbeat_at)}
                </div>
              </div>
            </button>
          ))
        )}
      </div>
    </div>
  )
}

function SummaryStat({ color, label, value }: { color: string; label: string; value: number }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
      <span style={{ width: 8, height: 8, borderRadius: '50%', background: color }} />
      <span style={{ fontVariantNumeric: 'tabular-nums', fontWeight: 600 }}>{value}</span>
      <span className="muted">{label}</span>
    </div>
  )
}
