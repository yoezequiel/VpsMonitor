import { useEffect, useState } from 'react'
import { listIncidentEvents, listIncidents, listServers } from '../api'
import type { Incident, IncidentEvent, Server } from '../types'

export function IncidentsView() {
  const [incidents, setIncidents] = useState<Incident[]>([])
  const [servers, setServers] = useState<Server[]>([])
  const [expanded, setExpanded] = useState<string | null>(null)

  useEffect(() => {
    function refresh() {
      listIncidents().then((data) => {
        data.sort((a, b) => b.started_at.localeCompare(a.started_at))
        setIncidents(data)
      })
    }
    refresh()
    listServers().then(setServers)
    const interval = setInterval(refresh, 30_000)
    return () => clearInterval(interval)
  }, [])

  const serverName = (id: string | null) => (id ? servers.find((s) => s.id === id)?.name ?? id.slice(0, 8) : '—')

  return (
    <div>
      <h1 style={{ fontSize: 20, margin: '0 0 16px' }}>Incidents</h1>

      <div className="card" style={{ padding: 0 }}>
        {incidents.length === 0 ? (
          <div style={{ padding: 16 }} className="muted">
            No incidents recorded.
          </div>
        ) : (
          incidents.map((incident) => (
            <div key={incident.id} style={{ borderBottom: '1px solid var(--border)' }}>
              <button
                onClick={() => setExpanded(expanded === incident.id ? null : incident.id)}
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  width: '100%',
                  padding: '12px 16px',
                  border: 'none',
                  background: 'none',
                  color: 'inherit',
                  textAlign: 'left',
                  cursor: 'pointer',
                }}
              >
                <div>
                  <div style={{ fontWeight: 600 }}>{incident.title}</div>
                  <div className="muted" style={{ fontSize: 12 }}>
                    {serverName(incident.server_id)} · started {new Date(incident.started_at).toLocaleString()}
                  </div>
                </div>
                <div style={{ textAlign: 'right' }}>
                  <span
                    style={{
                      fontSize: 11,
                      fontWeight: 600,
                      textTransform: 'uppercase',
                      color: incident.status === 'open' ? 'var(--status-critical)' : 'var(--status-good)',
                    }}
                  >
                    {incident.status}
                  </span>
                  <div className="muted" style={{ fontSize: 12 }}>
                    {incident.resolved_at
                      ? `${Math.round(
                          (new Date(incident.resolved_at).getTime() - new Date(incident.started_at).getTime()) / 60_000,
                        )}m`
                      : 'ongoing'}
                  </div>
                </div>
              </button>
              {expanded === incident.id && <IncidentTimeline id={incident.id} />}
            </div>
          ))
        )}
      </div>
    </div>
  )
}

function IncidentTimeline({ id }: { id: string }) {
  const [events, setEvents] = useState<IncidentEvent[] | null>(null)

  useEffect(() => {
    listIncidentEvents(id).then(setEvents)
  }, [id])

  if (!events) return <div style={{ padding: '0 16px 12px' }} className="muted">Loading…</div>
  if (events.length === 0) return <div style={{ padding: '0 16px 12px' }} className="muted">No timeline events.</div>

  return (
    <div style={{ padding: '0 16px 12px', display: 'grid', gap: 6 }}>
      {events
        .slice()
        .sort((a, b) => a.ts.localeCompare(b.ts))
        .map((e) => (
          <div key={e.id} style={{ fontSize: 13, display: 'flex', gap: 8 }}>
            <span className="muted" style={{ fontVariantNumeric: 'tabular-nums', flexShrink: 0 }}>
              {new Date(e.ts).toLocaleTimeString()}
            </span>
            <span>{e.description}</span>
          </div>
        ))}
    </div>
  )
}
