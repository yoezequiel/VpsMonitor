import { useEffect, useState } from 'react'
import { getServer, listMetrics } from '../api'
import type { Metric, Server } from '../types'
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
  const [rangeHours, setRangeHours] = useState(1)

  useEffect(() => {
    getServer(id).then(setServer)
  }, [id])

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
