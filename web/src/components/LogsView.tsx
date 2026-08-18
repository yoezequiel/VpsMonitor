import { useEffect, useState } from 'react'
import { listLogs, listServers } from '../api'
import type { LogEntry, LogLevel, LogSource, Server } from '../types'
import { inputStyle } from '../ui'

const LEVELS: LogLevel[] = ['trace', 'debug', 'info', 'warn', 'error']
const SOURCES: LogSource[] = ['agent', 'monitor', 'docker']

const LEVEL_COLOR: Record<LogLevel, string> = {
  trace: 'var(--text-muted)',
  debug: 'var(--text-muted)',
  info: 'var(--text-secondary)',
  warn: 'var(--status-warning)',
  error: 'var(--status-critical)',
}

export function LogsView() {
  const [servers, setServers] = useState<Server[]>([])
  const [serverId, setServerId] = useState('')
  const [level, setLevel] = useState<LogLevel | ''>('')
  const [source, setSource] = useState<LogSource | ''>('')
  const [q, setQ] = useState('')
  const [entries, setEntries] = useState<LogEntry[] | null>(null)

  useEffect(() => {
    listServers().then(setServers)
  }, [])

  useEffect(() => {
    let cancelled = false
    function refresh() {
      listLogs({
        serverId: serverId || undefined,
        level: level || undefined,
        source: source || undefined,
        q: q.trim() || undefined,
        sinceHours: 24,
        limit: 300,
      }).then((data) => {
        if (!cancelled) setEntries(data)
      })
    }
    refresh()
    const interval = setInterval(refresh, 15_000)
    return () => {
      cancelled = true
      clearInterval(interval)
    }
  }, [serverId, level, source, q])

  const serverName = (id: string) => servers.find((s) => s.id === id)?.name ?? id.slice(0, 8)

  return (
    <div>
      <h1 style={{ fontSize: 20, margin: '0 0 16px' }}>Logs</h1>

      <div className="card" style={{ display: 'flex', flexWrap: 'wrap', gap: 10, marginBottom: 16 }}>
        <select value={serverId} onChange={(e) => setServerId(e.target.value)} style={inputStyle}>
          <option value="">All servers</option>
          {servers.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>
        <select value={source} onChange={(e) => setSource(e.target.value as LogSource | '')} style={inputStyle}>
          <option value="">All sources</option>
          {SOURCES.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
        <select value={level} onChange={(e) => setLevel(e.target.value as LogLevel | '')} style={inputStyle}>
          <option value="">All levels</option>
          {LEVELS.map((l) => (
            <option key={l} value={l}>
              {l} and above
            </option>
          ))}
        </select>
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="search message…"
          style={{ ...inputStyle, flex: 1, minWidth: 160 }}
        />
      </div>

      <div className="card" style={{ padding: 0, fontFamily: 'ui-monospace, monospace', fontSize: 12.5 }}>
        {entries === null ? (
          <div style={{ padding: 16 }} className="muted">
            Loading…
          </div>
        ) : entries.length === 0 ? (
          <div style={{ padding: 16 }} className="muted">
            No log entries match these filters in the last 24h.
          </div>
        ) : (
          entries.map((entry) => (
            <div
              key={entry.id}
              style={{
                display: 'flex',
                gap: 10,
                padding: '6px 12px',
                borderBottom: '1px solid var(--border)',
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-word',
              }}
            >
              <span className="muted" style={{ flexShrink: 0 }}>
                {new Date(entry.ts).toLocaleTimeString()}
              </span>
              <span style={{ color: LEVEL_COLOR[entry.level], fontWeight: 600, flexShrink: 0, width: 44 }}>
                {entry.level.toUpperCase()}
              </span>
              <span className="secondary" style={{ flexShrink: 0 }}>
                {serverName(entry.server_id)}/{entry.source}
                {entry.container_name ? `/${entry.container_name}` : ''}
              </span>
              <span>{entry.message}</span>
            </div>
          ))
        )}
      </div>
    </div>
  )
}
