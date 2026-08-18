import type {
  AlertEvent,
  AlertRule,
  DockerContainer,
  Group,
  HealthCheck,
  HealthCheckResult,
  Incident,
  IncidentEvent,
  LogEntry,
  LogLevel,
  LogSource,
  Metric,
  NotificationChannel,
  Server,
  ServerStatusEvent,
} from './types'

const TOKEN_KEY = 'vpsmonitor.session_token'

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY)
}

function setToken(token: string) {
  localStorage.setItem(TOKEN_KEY, token)
}

export function clearToken() {
  localStorage.removeItem(TOKEN_KEY)
}

class ApiError extends Error {
  status: number

  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const token = getToken()
  const headers = new Headers(init?.headers)
  headers.set('content-type', 'application/json')
  if (token) headers.set('authorization', `Bearer ${token}`)

  const res = await fetch(path, { ...init, headers })
  if (res.status === 401) {
    clearToken()
    throw new ApiError(401, 'session expired')
  }
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }))
    throw new ApiError(res.status, body.error ?? res.statusText)
  }
  if (res.status === 204) return undefined as T
  return res.json() as Promise<T>
}

export async function login(username: string, password: string): Promise<void> {
  const { token } = await request<{ token: string }>('/api/auth/login', {
    method: 'POST',
    body: JSON.stringify({ username, password }),
  })
  setToken(token)
}

export async function logout(): Promise<void> {
  await request('/api/auth/logout', { method: 'POST' }).catch(() => {})
  clearToken()
}

export function listServers(): Promise<Server[]> {
  return request('/api/servers')
}

export function getServer(id: string): Promise<Server> {
  return request(`/api/servers/${id}`)
}

export function listMetrics(id: string, sinceHours = 1): Promise<Metric[]> {
  const since = new Date(Date.now() - sinceHours * 3600_000).toISOString()
  return request(`/api/servers/${id}/metrics?since=${encodeURIComponent(since)}`)
}

export function setServerTags(id: string, tags: string[]): Promise<void> {
  return request(`/api/servers/${id}/tags`, { method: 'POST', body: JSON.stringify({ tags }) })
}

export function listDockerContainers(serverId: string): Promise<DockerContainer[]> {
  return request(`/api/servers/${serverId}/docker`)
}

export function listHealthChecks(): Promise<HealthCheck[]> {
  return request('/api/healthchecks')
}

export function createHealthCheck(body: {
  name: string
  type: HealthCheck['type']
  target: string
  interval_s: number
  timeout_s: number
  expected_status?: number | null
  server_id?: string | null
}): Promise<HealthCheck> {
  return request('/api/healthchecks', { method: 'POST', body: JSON.stringify(body) })
}

export function deleteHealthCheck(id: string): Promise<void> {
  return request(`/api/healthchecks/${id}`, { method: 'DELETE' })
}

export function listHealthCheckResults(id: string, sinceHours = 24): Promise<HealthCheckResult[]> {
  const since = new Date(Date.now() - sinceHours * 3600_000).toISOString()
  return request(`/api/healthchecks/${id}/results?since=${encodeURIComponent(since)}`)
}

export function listAlertRules(): Promise<AlertRule[]> {
  return request('/api/alerts/rules')
}

export function createAlertRule(body: {
  name: string
  condition: AlertRule['condition']
  severity: AlertRule['severity']
  enabled: boolean
}): Promise<AlertRule> {
  return request('/api/alerts/rules', { method: 'POST', body: JSON.stringify(body) })
}

export function deleteAlertRule(id: string): Promise<void> {
  return request(`/api/alerts/rules/${id}`, { method: 'DELETE' })
}

export function listAlertEvents(sinceHours = 24 * 7): Promise<AlertEvent[]> {
  const since = new Date(Date.now() - sinceHours * 3600_000).toISOString()
  return request(`/api/alerts/events?since=${encodeURIComponent(since)}`)
}

export function listNotificationChannels(): Promise<NotificationChannel[]> {
  return request('/api/notifications/channels')
}

export function createNotificationChannel(body: {
  kind: NotificationChannel['kind']
  config: NotificationChannel['config']
  enabled: boolean
}): Promise<NotificationChannel> {
  return request('/api/notifications/channels', { method: 'POST', body: JSON.stringify(body) })
}

export function listIncidents(): Promise<Incident[]> {
  return request('/api/incidents')
}

export function getIncident(id: string): Promise<Incident> {
  return request(`/api/incidents/${id}`)
}

export function listIncidentEvents(id: string): Promise<IncidentEvent[]> {
  return request(`/api/incidents/${id}/events`)
}

export function listGroups(): Promise<Group[]> {
  return request('/api/groups')
}

export function createGroup(name: string): Promise<Group> {
  return request('/api/groups', { method: 'POST', body: JSON.stringify({ name }) })
}

export function deleteGroup(id: string): Promise<void> {
  return request(`/api/groups/${id}`, { method: 'DELETE' })
}

export function listGroupMembers(id: string): Promise<Server[]> {
  return request(`/api/groups/${id}/members`)
}

export function addGroupMember(groupId: string, serverId: string): Promise<void> {
  return request(`/api/groups/${groupId}/members/${serverId}`, { method: 'POST' })
}

export function removeGroupMember(groupId: string, serverId: string): Promise<void> {
  return request(`/api/groups/${groupId}/members/${serverId}`, { method: 'DELETE' })
}

export function listLogs(filter: {
  serverId?: string
  source?: LogSource
  level?: LogLevel
  q?: string
  sinceHours?: number
  limit?: number
}): Promise<LogEntry[]> {
  const params = new URLSearchParams()
  if (filter.serverId) params.set('server_id', filter.serverId)
  if (filter.source) params.set('source', filter.source)
  if (filter.level) params.set('level', filter.level)
  if (filter.q) params.set('q', filter.q)
  if (filter.limit) params.set('limit', String(filter.limit))
  params.set('since', new Date(Date.now() - (filter.sinceHours ?? 24) * 3600_000).toISOString())
  return request(`/api/logs?${params.toString()}`)
}

export function connectStatusStream(onEvent: (event: ServerStatusEvent) => void): () => void {
  const token = getToken()
  if (!token) return () => {}

  const proto = window.location.protocol === 'https:' ? 'wss' : 'ws'
  const ws = new WebSocket(`${proto}://${window.location.host}/api/ws?token=${encodeURIComponent(token)}`)
  ws.onmessage = (msg) => {
    try {
      onEvent(JSON.parse(msg.data))
    } catch {
      // ignore malformed frames
    }
  }
  return () => ws.close()
}
