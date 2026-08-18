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

export type ContainerState = 'running' | 'stopped' | 'restarting' | 'paused' | 'unknown'

export interface DockerContainer {
  server_id: string
  container_id: string
  name: string
  image: string
  state: ContainerState
  cpu_pct: number
  mem_used: number
  mem_limit: number
  net_rx: number
  net_tx: number
  restart_count: number
  started_at: string | null
  updated_at: string
}

export type HealthCheckType = 'http' | 'https' | 'tcp' | 'ping' | 'dns'

export interface HealthCheck {
  id: string
  name: string
  type: HealthCheckType
  target: string
  interval_s: number
  timeout_s: number
  expected_status: number | null
  server_id: string | null
}

export interface HealthCheckResult {
  id: string
  health_check_id: string
  ts: string
  status_code: number | null
  latency_ms: number
  ok: boolean
}

export type AlertSeverity = 'info' | 'warning' | 'critical'

export type AlertCondition =
  | { kind: 'cpu_above'; server_id: string | null; threshold_pct: number; for_secs: number }
  | { kind: 'mem_above'; server_id: string | null; threshold_pct: number; for_secs: number }
  | { kind: 'disk_above'; server_id: string | null; threshold_pct: number; for_secs: number }
  | { kind: 'server_offline'; server_id: string | null; for_secs: number }
  | { kind: 'container_stopped'; server_id: string | null }
  | { kind: 'health_check_failed'; health_check_id: string; for_secs: number }

export interface AlertRule {
  id: string
  name: string
  condition: AlertCondition
  severity: AlertSeverity
  enabled: boolean
}

export type AlertState = 'firing' | 'resolved'

export interface AlertEvent {
  id: string
  rule_id: string
  server_id: string | null
  state: AlertState
  started_at: string
  resolved_at: string | null
  notified_at: string | null
}

export type IncidentStatus = 'open' | 'resolved'

export interface Incident {
  id: string
  title: string
  started_at: string
  resolved_at: string | null
  status: IncidentStatus
  server_id: string | null
}

export interface IncidentEvent {
  id: string
  incident_id: string
  ts: string
  description: string
  source_ref: string | null
}

export type NotificationChannelKind = 'webhook' | 'email'

export interface WebhookConfig {
  url: string
}

export interface EmailConfig {
  smtp_host: string
  smtp_port: number
  username: string
  password: string
  from: string
  to: string
}

export interface NotificationChannel {
  id: string
  kind: NotificationChannelKind
  config: WebhookConfig | EmailConfig | Record<string, unknown>
  enabled: boolean
}

export interface Group {
  id: string
  name: string
}

export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error'
export type LogSource = 'agent' | 'monitor' | 'docker'

export interface LogEntry {
  id: string
  server_id: string
  source: LogSource
  container_name: string | null
  level: LogLevel
  message: string
  ts: string
}
