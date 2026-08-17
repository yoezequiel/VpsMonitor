export type ServerStatus = 'online' | 'offline' | 'warning' | 'unknown'

export interface Server {
  id: string
  name: string
  hostname: string
  ip: string | null
  os: string
  arch: string
  agent_version: string
  tags: string[]
  status: ServerStatus
  last_heartbeat_at: string | null
  created_at: string
}

export interface Metric {
  id: string
  server_id: string
  ts: string
  cpu_pct: number
  mem_used: number
  mem_total: number
  disk_used: number
  disk_total: number
  load1: number
  load5: number
  load15: number
  net_rx: number
  net_tx: number
  uptime_s: number
}

export interface ServerStatusEvent {
  server_id: string
  status: ServerStatus
}
