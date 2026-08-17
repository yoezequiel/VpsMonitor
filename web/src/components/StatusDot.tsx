import type { ServerStatus } from '../types'

const STATUS_META: Record<ServerStatus, { color: string; label: string }> = {
  online: { color: 'var(--status-good)', label: 'Online' },
  warning: { color: 'var(--status-warning)', label: 'Warning' },
  offline: { color: 'var(--status-critical)', label: 'Offline' },
  unknown: { color: 'var(--status-unknown)', label: 'Unknown' },
}

export function StatusDot({ status }: { status: ServerStatus }) {
  const meta = STATUS_META[status]
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
      <span
        aria-hidden
        style={{
          width: 8,
          height: 8,
          borderRadius: '50%',
          background: meta.color,
          flexShrink: 0,
        }}
      />
      <span>{meta.label}</span>
    </span>
  )
}
