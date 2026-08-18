import { useEffect, useState } from 'react'
import {
  createHealthCheck,
  deleteHealthCheck,
  listHealthCheckResults,
  listHealthChecks,
  listServers,
} from '../api'
import type { HealthCheck, HealthCheckResult, HealthCheckType, Server } from '../types'
import { Field, buttonStyle, deleteButtonStyle, inputStyle } from '../ui'

const TYPES: HealthCheckType[] = ['http', 'https', 'tcp', 'ping', 'dns']

export function HealthChecksView() {
  const [checks, setChecks] = useState<HealthCheck[]>([])
  const [servers, setServers] = useState<Server[]>([])
  const [expanded, setExpanded] = useState<string | null>(null)

  function refresh() {
    listHealthChecks().then(setChecks)
  }

  useEffect(() => {
    refresh()
    listServers().then(setServers)
    const interval = setInterval(refresh, 30_000)
    return () => clearInterval(interval)
  }, [])

  async function handleDelete(id: string) {
    await deleteHealthCheck(id)
    refresh()
  }

  const serverName = (id: string | null) => (id ? servers.find((s) => s.id === id)?.name ?? id.slice(0, 8) : 'all servers')

  return (
    <div>
      <h1 style={{ fontSize: 20, margin: '0 0 16px' }}>Health checks</h1>

      <CreateHealthCheckForm servers={servers} onCreated={refresh} />

      <div className="card" style={{ padding: 0, marginTop: 16 }}>
        {checks.length === 0 ? (
          <div style={{ padding: 16 }} className="muted">
            No health checks configured yet.
          </div>
        ) : (
          checks.map((check) => (
            <div key={check.id} style={{ borderBottom: '1px solid var(--border)' }}>
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  padding: '12px 16px',
                  gap: 12,
                }}
              >
                <button
                  onClick={() => setExpanded(expanded === check.id ? null : check.id)}
                  style={{
                    background: 'none',
                    border: 'none',
                    color: 'inherit',
                    textAlign: 'left',
                    cursor: 'pointer',
                    padding: 0,
                    flex: 1,
                  }}
                >
                  <div style={{ fontWeight: 600 }}>
                    {check.name} <span className="muted" style={{ fontWeight: 400 }}>({check.type})</span>
                  </div>
                  <div className="muted" style={{ fontSize: 12 }}>
                    {check.target} · every {check.interval_s}s · {serverName(check.server_id)}
                  </div>
                </button>
                <button onClick={() => handleDelete(check.id)} style={deleteButtonStyle}>
                  Delete
                </button>
              </div>
              {expanded === check.id && <HealthCheckResults id={check.id} />}
            </div>
          ))
        )}
      </div>
    </div>
  )
}

function HealthCheckResults({ id }: { id: string }) {
  const [results, setResults] = useState<HealthCheckResult[] | null>(null)

  useEffect(() => {
    listHealthCheckResults(id, 24).then(setResults)
  }, [id])

  if (!results) return <div style={{ padding: '0 16px 12px' }} className="muted">Loading…</div>
  if (results.length === 0) return <div style={{ padding: '0 16px 12px' }} className="muted">No results in the last 24h.</div>

  const okCount = results.filter((r) => r.ok).length
  const uptimePct = (okCount / results.length) * 100
  const recent = [...results].slice(-10).reverse()

  return (
    <div style={{ padding: '0 16px 12px' }}>
      <div className="muted" style={{ fontSize: 12, marginBottom: 6 }}>
        {uptimePct.toFixed(1)}% uptime over last 24h ({results.length} checks)
      </div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
        {recent.map((r) => (
          <span
            key={r.id}
            title={`${new Date(r.ts).toLocaleString()} · ${r.latency_ms}ms${r.status_code ? ` · HTTP ${r.status_code}` : ''}`}
            style={{
              width: 10,
              height: 10,
              borderRadius: 2,
              background: r.ok ? 'var(--status-good)' : 'var(--status-critical)',
            }}
          />
        ))}
      </div>
    </div>
  )
}

function CreateHealthCheckForm({ servers, onCreated }: { servers: Server[]; onCreated: () => void }) {
  const [name, setName] = useState('')
  const [type, setType] = useState<HealthCheckType>('http')
  const [target, setTarget] = useState('')
  const [intervalS, setIntervalS] = useState(30)
  const [timeoutS, setTimeoutS] = useState(5)
  const [expectedStatus, setExpectedStatus] = useState('')
  const [serverId, setServerId] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      await createHealthCheck({
        name,
        type,
        target,
        interval_s: intervalS,
        timeout_s: timeoutS,
        expected_status:
          (type === 'http' || type === 'https') && expectedStatus ? Number(expectedStatus) : null,
        server_id: serverId || null,
      })
      setName('')
      setTarget('')
      setExpectedStatus('')
      onCreated()
    } catch {
      setError('Could not create health check')
    } finally {
      setBusy(false)
    }
  }

  return (
    <form onSubmit={handleSubmit} className="card" style={{ display: 'grid', gap: 10, gridTemplateColumns: 'repeat(6, 1fr)', alignItems: 'end' }}>
      <Field label="Name" span={2}>
        <input value={name} onChange={(e) => setName(e.target.value)} required style={inputStyle} />
      </Field>
      <Field label="Type">
        <select value={type} onChange={(e) => setType(e.target.value as HealthCheckType)} style={inputStyle}>
          {TYPES.map((t) => (
            <option key={t} value={t}>
              {t.toUpperCase()}
            </option>
          ))}
        </select>
      </Field>
      <Field label="Target" span={2}>
        <input
          value={target}
          onChange={(e) => setTarget(e.target.value)}
          placeholder={type === 'http' || type === 'https' ? 'https://example.com' : 'host:port or host'}
          required
          style={inputStyle}
        />
      </Field>
      <Field label="Scope">
        <select value={serverId} onChange={(e) => setServerId(e.target.value)} style={inputStyle}>
          <option value="">All servers</option>
          {servers.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>
      </Field>
      <Field label="Interval (s)">
        <input
          type="number"
          min={5}
          value={intervalS}
          onChange={(e) => setIntervalS(Number(e.target.value))}
          style={inputStyle}
        />
      </Field>
      <Field label="Timeout (s)">
        <input
          type="number"
          min={1}
          value={timeoutS}
          onChange={(e) => setTimeoutS(Number(e.target.value))}
          style={inputStyle}
        />
      </Field>
      {(type === 'http' || type === 'https') && (
        <Field label="Expected status">
          <input
            type="number"
            placeholder="200"
            value={expectedStatus}
            onChange={(e) => setExpectedStatus(e.target.value)}
            style={inputStyle}
          />
        </Field>
      )}
      <button type="submit" disabled={busy} style={buttonStyle}>
        {busy ? 'Adding…' : 'Add check'}
      </button>
      {error && (
        <div style={{ gridColumn: '1 / -1', color: 'var(--status-critical)', fontSize: 13 }}>{error}</div>
      )}
    </form>
  )
}

