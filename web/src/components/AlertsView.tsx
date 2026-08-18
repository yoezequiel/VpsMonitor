import { useEffect, useState } from 'react'
import {
  createAlertRule,
  createNotificationChannel,
  deleteAlertRule,
  listAlertEvents,
  listAlertRules,
  listHealthChecks,
  listNotificationChannels,
  listServers,
} from '../api'
import type {
  AlertCondition,
  AlertEvent,
  AlertRule,
  AlertSeverity,
  HealthCheck,
  NotificationChannel,
  NotificationChannelKind,
  Server,
} from '../types'
import { Field, buttonStyle, deleteButtonStyle, inputStyle } from '../ui'

const CONDITION_KINDS: AlertCondition['kind'][] = [
  'cpu_above',
  'mem_above',
  'disk_above',
  'server_offline',
  'container_stopped',
  'health_check_failed',
]

const CONDITION_LABEL: Record<AlertCondition['kind'], string> = {
  cpu_above: 'CPU above threshold',
  mem_above: 'Memory above threshold',
  disk_above: 'Disk above threshold',
  server_offline: 'Server offline',
  container_stopped: 'Container stopped',
  health_check_failed: 'Health check failed',
}

const SEVERITIES: AlertSeverity[] = ['info', 'warning', 'critical']

function describeCondition(c: AlertCondition, servers: Server[], checks: HealthCheck[]): string {
  const serverName = (id: string | null) =>
    id ? servers.find((s) => s.id === id)?.name ?? id.slice(0, 8) : 'any server'
  switch (c.kind) {
    case 'cpu_above':
      return `CPU > ${c.threshold_pct}% for ${c.for_secs}s on ${serverName(c.server_id)}`
    case 'mem_above':
      return `Memory > ${c.threshold_pct}% for ${c.for_secs}s on ${serverName(c.server_id)}`
    case 'disk_above':
      return `Disk > ${c.threshold_pct}% for ${c.for_secs}s on ${serverName(c.server_id)}`
    case 'server_offline':
      return `${serverName(c.server_id)} offline for ${c.for_secs}s`
    case 'container_stopped':
      return `Any container stopped on ${serverName(c.server_id)}`
    case 'health_check_failed':
      return `Health check "${checks.find((h) => h.id === c.health_check_id)?.name ?? c.health_check_id.slice(0, 8)}" failing for ${c.for_secs}s`
  }
}

export function AlertsView() {
  const [rules, setRules] = useState<AlertRule[]>([])
  const [events, setEvents] = useState<AlertEvent[]>([])
  const [channels, setChannels] = useState<NotificationChannel[]>([])
  const [servers, setServers] = useState<Server[]>([])
  const [checks, setChecks] = useState<HealthCheck[]>([])

  function refresh() {
    listAlertRules().then(setRules)
    listAlertEvents(24 * 7).then(setEvents)
    listNotificationChannels().then(setChannels)
  }

  useEffect(() => {
    refresh()
    listServers().then(setServers)
    listHealthChecks().then(setChecks)
    const interval = setInterval(refresh, 30_000)
    return () => clearInterval(interval)
  }, [])

  async function handleDelete(id: string) {
    await deleteAlertRule(id)
    refresh()
  }

  const firing = events.filter((e) => e.state === 'firing').length

  return (
    <div>
      <h1 style={{ fontSize: 20, margin: '0 0 16px' }}>Alerts</h1>

      <div className="card" style={{ marginBottom: 16 }}>
        <strong>{firing}</strong> <span className="muted">alert(s) currently firing</span>
        <span className="muted"> · {events.length} event(s) in the last 7 days</span>
      </div>

      <CreateAlertRuleForm servers={servers} checks={checks} onCreated={refresh} />

      <div className="card" style={{ padding: 0, marginTop: 16 }}>
        {rules.length === 0 ? (
          <div style={{ padding: 16 }} className="muted">
            No alert rules configured yet.
          </div>
        ) : (
          rules.map((rule) => {
            const condition = rule.condition
            return (
              <div
                key={rule.id}
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  padding: '12px 16px',
                  borderBottom: '1px solid var(--border)',
                  gap: 12,
                }}
              >
                <div>
                  <div style={{ fontWeight: 600 }}>
                    {rule.name}{' '}
                    <span className="muted" style={{ fontWeight: 400 }}>
                      ({rule.severity}{!rule.enabled ? ', disabled' : ''})
                    </span>
                  </div>
                  <div className="muted" style={{ fontSize: 12 }}>
                    {describeCondition(condition, servers, checks)}
                  </div>
                </div>
                <button onClick={() => handleDelete(rule.id)} style={deleteButtonStyle}>
                  Delete
                </button>
              </div>
            )
          })
        )}
      </div>

      <NotificationChannels channels={channels} onCreated={refresh} />

      <div className="card" style={{ padding: 0, marginTop: 16 }}>
        <h2 style={{ fontSize: 15, margin: 0, padding: '12px 16px', borderBottom: '1px solid var(--border)' }}>
          Recent alert events
        </h2>
        {events.length === 0 ? (
          <div style={{ padding: 16 }} className="muted">
            No alert events in the last 7 days.
          </div>
        ) : (
          events
            .slice()
            .sort((a, b) => b.started_at.localeCompare(a.started_at))
            .map((e) => {
              const rule = rules.find((r) => r.id === e.rule_id)
              const server = servers.find((s) => s.id === e.server_id)
              return (
                <div
                  key={e.id}
                  style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    padding: '10px 16px',
                    borderBottom: '1px solid var(--border)',
                    fontSize: 13,
                  }}
                >
                  <span>
                    <span
                      style={{
                        color: e.state === 'firing' ? 'var(--status-critical)' : 'var(--status-good)',
                        fontWeight: 600,
                        marginRight: 8,
                        textTransform: 'uppercase',
                        fontSize: 11,
                      }}
                    >
                      {e.state}
                    </span>
                    {rule?.name ?? e.rule_id.slice(0, 8)}
                    {server ? ` · ${server.name}` : ''}
                  </span>
                  <span className="muted">{new Date(e.started_at).toLocaleString()}</span>
                </div>
              )
            })
        )}
      </div>
    </div>
  )
}

function CreateAlertRuleForm({
  servers,
  checks,
  onCreated,
}: {
  servers: Server[]
  checks: HealthCheck[]
  onCreated: () => void
}) {
  const [name, setName] = useState('')
  const [kind, setKind] = useState<AlertCondition['kind']>('cpu_above')
  const [severity, setSeverity] = useState<AlertSeverity>('warning')
  const [serverId, setServerId] = useState('')
  const [thresholdPct, setThresholdPct] = useState(90)
  const [forSecs, setForSecs] = useState(300)
  const [healthCheckId, setHealthCheckId] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setError(null)

    let condition: AlertCondition
    switch (kind) {
      case 'cpu_above':
      case 'mem_above':
      case 'disk_above':
        condition = { kind, server_id: serverId || null, threshold_pct: thresholdPct, for_secs: forSecs }
        break
      case 'server_offline':
        condition = { kind, server_id: serverId || null, for_secs: forSecs }
        break
      case 'container_stopped':
        condition = { kind, server_id: serverId || null }
        break
      case 'health_check_failed':
        if (!healthCheckId) {
          setError('Pick a health check')
          return
        }
        condition = { kind, health_check_id: healthCheckId, for_secs: forSecs }
        break
    }

    setBusy(true)
    try {
      await createAlertRule({ name, condition, severity, enabled: true })
      setName('')
      onCreated()
    } catch {
      setError('Could not create alert rule')
    } finally {
      setBusy(false)
    }
  }

  const needsThreshold = kind === 'cpu_above' || kind === 'mem_above' || kind === 'disk_above'
  const needsForSecs = kind !== 'container_stopped'
  const needsServer = kind !== 'health_check_failed'

  return (
    <form onSubmit={handleSubmit} className="card" style={{ display: 'grid', gap: 10, gridTemplateColumns: 'repeat(6, 1fr)', alignItems: 'end' }}>
      <Field label="Name" span={2}>
        <input value={name} onChange={(e) => setName(e.target.value)} required style={inputStyle} />
      </Field>
      <Field label="Condition" span={2}>
        <select value={kind} onChange={(e) => setKind(e.target.value as AlertCondition['kind'])} style={inputStyle}>
          {CONDITION_KINDS.map((k) => (
            <option key={k} value={k}>
              {CONDITION_LABEL[k]}
            </option>
          ))}
        </select>
      </Field>
      <Field label="Severity">
        <select value={severity} onChange={(e) => setSeverity(e.target.value as AlertSeverity)} style={inputStyle}>
          {SEVERITIES.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
      </Field>
      {needsServer && (
        <Field label="Scope">
          <select value={serverId} onChange={(e) => setServerId(e.target.value)} style={inputStyle}>
            <option value="">Any server</option>
            {servers.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        </Field>
      )}
      {kind === 'health_check_failed' && (
        <Field label="Health check" span={2}>
          <select value={healthCheckId} onChange={(e) => setHealthCheckId(e.target.value)} style={inputStyle}>
            <option value="">Select…</option>
            {checks.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </Field>
      )}
      {needsThreshold && (
        <Field label="Threshold %">
          <input
            type="number"
            min={1}
            max={100}
            value={thresholdPct}
            onChange={(e) => setThresholdPct(Number(e.target.value))}
            style={inputStyle}
          />
        </Field>
      )}
      {needsForSecs && (
        <Field label="For (s)">
          <input
            type="number"
            min={0}
            value={forSecs}
            onChange={(e) => setForSecs(Number(e.target.value))}
            style={inputStyle}
          />
        </Field>
      )}
      <button type="submit" disabled={busy} style={buttonStyle}>
        {busy ? 'Adding…' : 'Add rule'}
      </button>
      {error && (
        <div style={{ gridColumn: '1 / -1', color: 'var(--status-critical)', fontSize: 13 }}>{error}</div>
      )}
    </form>
  )
}

function NotificationChannels({
  channels,
  onCreated,
}: {
  channels: NotificationChannel[]
  onCreated: () => void
}) {
  const [kind, setKind] = useState<NotificationChannelKind>('webhook')
  const [url, setUrl] = useState('')
  const [smtpHost, setSmtpHost] = useState('')
  const [smtpPort, setSmtpPort] = useState(587)
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [from, setFrom] = useState('')
  const [to, setTo] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      const config =
        kind === 'webhook'
          ? { url }
          : { smtp_host: smtpHost, smtp_port: smtpPort, username, password, from, to }
      await createNotificationChannel({ kind, config, enabled: true })
      setUrl('')
      onCreated()
    } catch {
      setError('Could not create notification channel')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div style={{ marginTop: 16 }}>
      <h2 style={{ fontSize: 15, margin: '0 0 8px' }}>Notification channels</h2>
      <form onSubmit={handleSubmit} className="card" style={{ display: 'grid', gap: 10, gridTemplateColumns: 'repeat(6, 1fr)', alignItems: 'end', marginBottom: 12 }}>
        <Field label="Kind">
          <select value={kind} onChange={(e) => setKind(e.target.value as NotificationChannelKind)} style={inputStyle}>
            <option value="webhook">Webhook</option>
            <option value="email">Email</option>
          </select>
        </Field>
        {kind === 'webhook' ? (
          <Field label="Webhook URL" span={4}>
            <input value={url} onChange={(e) => setUrl(e.target.value)} required style={inputStyle} placeholder="https://…" />
          </Field>
        ) : (
          <>
            <Field label="SMTP host">
              <input value={smtpHost} onChange={(e) => setSmtpHost(e.target.value)} required style={inputStyle} />
            </Field>
            <Field label="Port">
              <input type="number" value={smtpPort} onChange={(e) => setSmtpPort(Number(e.target.value))} style={inputStyle} />
            </Field>
            <Field label="Username">
              <input value={username} onChange={(e) => setUsername(e.target.value)} style={inputStyle} />
            </Field>
            <Field label="Password">
              <input type="password" value={password} onChange={(e) => setPassword(e.target.value)} style={inputStyle} />
            </Field>
            <Field label="From">
              <input value={from} onChange={(e) => setFrom(e.target.value)} required style={inputStyle} />
            </Field>
            <Field label="To">
              <input value={to} onChange={(e) => setTo(e.target.value)} required style={inputStyle} />
            </Field>
          </>
        )}
        <button type="submit" disabled={busy} style={buttonStyle}>
          {busy ? 'Adding…' : 'Add channel'}
        </button>
        {error && (
          <div style={{ gridColumn: '1 / -1', color: 'var(--status-critical)', fontSize: 13 }}>{error}</div>
        )}
      </form>
      <div className="card" style={{ padding: 0 }}>
        {channels.length === 0 ? (
          <div style={{ padding: 16 }} className="muted">
            No notification channels configured.
          </div>
        ) : (
          channels.map((c) => (
            <div key={c.id} style={{ padding: '10px 16px', borderBottom: '1px solid var(--border)', fontSize: 13 }}>
              <strong style={{ textTransform: 'capitalize' }}>{c.kind}</strong>{' '}
              <span className="muted">
                {c.kind === 'webhook' ? (c.config as { url: string }).url : (c.config as { to: string }).to}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  )
}
