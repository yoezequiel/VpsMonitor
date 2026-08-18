import { useEffect, useState } from 'react'
import { getServer, listDockerContainers, listMetrics, setServerTags } from '../api'
import type { ContainerState, DockerContainer, Metric, Server } from '../types'
import { StatusDot } from './StatusDot'
import { LineChart } from './LineChart'

const RANGES = [
  { label: '1h', hours: 1 },
  { label: '6h', hours: 6 },
  { label: '24h', hours: 24 },
  { label: '7d', hours: 24 * 7 },
]

function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  return `${days}d ${hours}h`
}

function formatBytes(bytes: number): string {
  const gb = bytes / 1024 ** 3
  return gb >= 1 ? `${gb.toFixed(1)} GB` : `${(bytes / 1024 ** 2).toFixed(0)} MB`
}

export function ServerDetail({ id, onBack }: { id: string; onBack: () => void }) {
  const [server, setServer] = useState<Server | null>(null)
  const [metrics, setMetrics] = useState<Metric[]>([])
  const [containers, setContainers] = useState<DockerContainer[]>([])
  const [rangeHours, setRangeHours] = useState(1)

  function refreshServer() {
    getServer(id).then(setServer)
  }

  useEffect(refreshServer, [id])

  useEffect(() => {
    let cancelled = false
    listMetrics(id, rangeHours).then((data) => {
      if (!cancelled) setMetrics(data)
    })
    const interval = setInterval(() => {
      listMetrics(id, rangeHours).then((data) => {
        if (!cancelled) setMetrics(data)
      })
    }, 30_000)
    return () => {
      cancelled = true
      clearInterval(interval)
    }
  }, [id, rangeHours])

  useEffect(() => {
    let cancelled = false
    function refresh() {
      listDockerContainers(id).then((data) => {
        if (!cancelled) setContainers(data)
      })
    }
    refresh()
    const interval = setInterval(refresh, 30_000)
    return () => {
      cancelled = true
      clearInterval(interval)
    }
  }, [id])

  if (!server) return null

  const latest = metrics.at(-1)

  return (
    <div>
      <button
        onClick={onBack}
        style={{ background: 'none', border: 'none', color: 'var(--series-blue)', cursor: 'pointer', padding: 0, marginBottom: 16 }}
      >
        ← Back to Infrastructure
      </button>

      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 16 }}>
        <div>
          <h1 style={{ fontSize: 22, margin: 0 }}>{server.name}</h1>
          <div className="muted" style={{ fontSize: 13 }}>
            {server.hostname} · {server.os} · {server.arch} · Agent {server.agent_version}
          </div>
        </div>
        <StatusDot status={server.status} />
      </div>

      <TagsEditor server={server} onChanged={refreshServer} />

      <div className="card" style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 16, marginBottom: 16 }}>
        <StatTile label="CPU" value={latest ? `${latest.cpu_pct.toFixed(0)}%` : '—'} />
        <StatTile
          label="Memory"
          value={latest ? `${((latest.mem_used / latest.mem_total) * 100).toFixed(0)}%` : '—'}
          sub={latest ? `${formatBytes(latest.mem_used)} / ${formatBytes(latest.mem_total)}` : undefined}
        />
        <StatTile
          label="Disk"
          value={latest ? `${((latest.disk_used / latest.disk_total) * 100).toFixed(0)}%` : '—'}
          sub={latest ? `${formatBytes(latest.disk_used)} / ${formatBytes(latest.disk_total)}` : undefined}
        />
        <StatTile
          label="Uptime"
          value={latest ? formatUptime(latest.uptime_s) : '—'}
          sub={latest ? `load ${latest.load1.toFixed(2)}` : undefined}
        />
      </div>

      <div style={{ display: 'flex', gap: 8, marginBottom: 12 }}>
        {RANGES.map((r) => (
          <button
            key={r.label}
            onClick={() => setRangeHours(r.hours)}
            style={{
              padding: '4px 10px',
              borderRadius: 999,
              border: '1px solid var(--border)',
              background: rangeHours === r.hours ? 'var(--series-blue)' : 'var(--surface-1)',
              color: rangeHours === r.hours ? '#fff' : 'var(--text-secondary)',
              cursor: 'pointer',
              fontSize: 13,
            }}
          >
            {r.label}
          </button>
        ))}
      </div>

      <div className="card" style={{ display: 'grid', gap: 24 }}>
        <LineChart title="CPU" unit="%" data={metrics.map((m) => ({ ts: m.ts, value: m.cpu_pct }))} />
        <LineChart
          title="Memory"
          unit="%"
          data={metrics.map((m) => ({ ts: m.ts, value: (m.mem_used / m.mem_total) * 100 }))}
        />
      </div>

      {containers.length > 0 && (
        <div className="card" style={{ marginTop: 16, padding: 0 }}>
          <h2 style={{ fontSize: 15, margin: 0, padding: '12px 16px', borderBottom: '1px solid var(--border)' }}>
            Docker containers
          </h2>
          {containers.map((c) => (
            <div
              key={c.container_id}
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                padding: '10px 16px',
                borderBottom: '1px solid var(--border)',
                gap: 12,
              }}
            >
              <div>
                <div style={{ fontWeight: 600 }}>{c.name}</div>
                <div className="muted" style={{ fontSize: 12 }}>
                  {c.image}
                  {c.restart_count > 0 ? ` · ${c.restart_count} restart(s)` : ''}
                </div>
              </div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 16, fontSize: 13 }}>
                <span className="secondary">{c.cpu_pct.toFixed(0)}% CPU</span>
                <span className="secondary">{formatBytes(c.mem_used)} mem</span>
                <ContainerStateDot state={c.state} />
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function StatTile({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div>
      <div className="muted" style={{ fontSize: 12 }}>
        {label}
      </div>
      <div style={{ fontSize: 24, fontWeight: 600 }}>{value}</div>
      {sub && (
        <div className="muted" style={{ fontSize: 11 }}>
          {sub}
        </div>
      )}
    </div>
  )
}

const CONTAINER_STATE_COLOR: Record<ContainerState, string> = {
  running: 'var(--status-good)',
  restarting: 'var(--status-warning)',
  paused: 'var(--status-warning)',
  stopped: 'var(--status-critical)',
  unknown: 'var(--status-unknown)',
}

function ContainerStateDot({ state }: { state: ContainerState }) {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
      <span
        aria-hidden
        style={{ width: 7, height: 7, borderRadius: '50%', background: CONTAINER_STATE_COLOR[state] }}
      />
      <span className="secondary" style={{ textTransform: 'capitalize' }}>
        {state}
      </span>
    </span>
  )
}

function TagsEditor({ server, onChanged }: { server: Server; onChanged: () => void }) {
  const [draft, setDraft] = useState('')
  const [busy, setBusy] = useState(false)

  async function commit(tags: string[]) {
    setBusy(true)
    try {
      await setServerTags(server.id, tags)
      onChanged()
    } finally {
      setBusy(false)
    }
  }

  function addTag(e: React.FormEvent) {
    e.preventDefault()
    const tag = draft.trim()
    if (!tag || server.tags.includes(tag)) {
      setDraft('')
      return
    }
    setDraft('')
    commit([...server.tags, tag])
  }

  function removeTag(tag: string) {
    commit(server.tags.filter((t) => t !== tag))
  }

  return (
    <form
      onSubmit={addTag}
      style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 6, marginBottom: 16 }}
    >
      {server.tags.map((tag) => (
        <span
          key={tag}
          style={{
            display: 'inline-flex',
            alignItems: 'center',
            gap: 6,
            padding: '3px 8px',
            borderRadius: 999,
            border: '1px solid var(--border)',
            background: 'var(--surface-1)',
            fontSize: 12,
          }}
        >
          {tag}
          <button
            type="button"
            onClick={() => removeTag(tag)}
            disabled={busy}
            aria-label={`Remove tag ${tag}`}
            style={{ background: 'none', border: 'none', color: 'var(--text-muted)', cursor: 'pointer', padding: 0, lineHeight: 1 }}
          >
            ×
          </button>
        </span>
      ))}
      <input
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        placeholder="add tag…"
        disabled={busy}
        style={{
          border: '1px dashed var(--border)',
          borderRadius: 999,
          padding: '3px 8px',
          fontSize: 12,
          background: 'transparent',
          color: 'inherit',
          width: 90,
        }}
      />
    </form>
  )
}
