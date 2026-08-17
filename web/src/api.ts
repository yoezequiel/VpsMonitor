import type { Metric, Server, ServerStatusEvent } from './types'

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
