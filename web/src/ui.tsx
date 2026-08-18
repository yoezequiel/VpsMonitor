export function Field({ label, span, children }: { label: string; span?: number; children: React.ReactNode }) {
  return (
    <label style={{ display: 'grid', gap: 4, gridColumn: span ? `span ${span}` : undefined }}>
      <span className="secondary" style={{ fontSize: 12 }}>
        {label}
      </span>
      {children}
    </label>
  )
}

export const inputStyle: React.CSSProperties = {
  padding: '6px 8px',
  borderRadius: 6,
  border: '1px solid var(--border)',
  background: 'var(--page)',
  color: 'var(--text-primary)',
  fontSize: 13,
}

export const buttonStyle: React.CSSProperties = {
  padding: '7px 12px',
  borderRadius: 6,
  border: 'none',
  background: 'var(--series-blue)',
  color: '#fff',
  fontWeight: 600,
  cursor: 'pointer',
  fontSize: 13,
}

export const deleteButtonStyle: React.CSSProperties = {
  padding: '4px 10px',
  borderRadius: 6,
  border: '1px solid var(--border)',
  background: 'none',
  color: 'var(--status-critical)',
  cursor: 'pointer',
  fontSize: 12,
}
