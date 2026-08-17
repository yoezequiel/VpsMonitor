import { useMemo, useRef, useState } from 'react'

interface Point {
  ts: string
  value: number
}

interface LineChartProps {
  title: string
  unit: string
  data: Point[]
  height?: number
  formatValue?: (v: number) => string
}

const WIDTH = 600

export function LineChart({ title, unit, data, height = 140, formatValue }: LineChartProps) {
  const [hoverIndex, setHoverIndex] = useState<number | null>(null)
  const svgRef = useRef<SVGSVGElement>(null)

  const fmt = formatValue ?? ((v: number) => `${v.toFixed(1)}${unit}`)

  const { path, area, points, min, max } = useMemo(() => {
    if (data.length === 0) {
      return { path: '', area: '', points: [] as { x: number; y: number }[], min: 0, max: 0 }
    }
    const values = data.map((d) => d.value)
    const min = Math.min(...values, 0)
    const max = Math.max(...values, 1)
    const span = max - min || 1
    const padTop = 12
    const padBottom = 12
    const plotHeight = height - padTop - padBottom

    const points = data.map((d, i) => {
      const x = data.length === 1 ? WIDTH : (i / (data.length - 1)) * WIDTH
      const y = padTop + plotHeight - ((d.value - min) / span) * plotHeight
      return { x, y }
    })

    const path = points.map((p, i) => `${i === 0 ? 'M' : 'L'}${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' ')
    const area = `${path} L${points[points.length - 1].x.toFixed(1)},${height} L${points[0].x.toFixed(1)},${height} Z`

    return { path, area, points, min, max }
  }, [data, height])

  function handleMove(e: React.MouseEvent<SVGSVGElement>) {
    if (!svgRef.current || points.length === 0) return
    const rect = svgRef.current.getBoundingClientRect()
    const relX = ((e.clientX - rect.left) / rect.width) * WIDTH
    let nearest = 0
    let nearestDist = Infinity
    points.forEach((p, i) => {
      const dist = Math.abs(p.x - relX)
      if (dist < nearestDist) {
        nearestDist = dist
        nearest = i
      }
    })
    setHoverIndex(nearest)
  }

  const hovered = hoverIndex !== null ? { point: points[hoverIndex], datum: data[hoverIndex] } : null

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
        <span className="secondary" style={{ fontSize: 13 }}>
          {title}
        </span>
        {hovered ? (
          <span style={{ fontSize: 13, fontVariantNumeric: 'tabular-nums' }}>
            {fmt(hovered.datum.value)} <span className="muted">· {new Date(hovered.datum.ts).toLocaleTimeString()}</span>
          </span>
        ) : (
          <span className="muted" style={{ fontSize: 13, fontVariantNumeric: 'tabular-nums' }}>
            {fmt(max)}
          </span>
        )}
      </div>
      {data.length === 0 ? (
        <div className="muted" style={{ height, display: 'flex', alignItems: 'center', fontSize: 13 }}>
          No data yet
        </div>
      ) : (
        <svg
          ref={svgRef}
          viewBox={`0 0 ${WIDTH} ${height}`}
          width="100%"
          height={height}
          onMouseMove={handleMove}
          onMouseLeave={() => setHoverIndex(null)}
          style={{ display: 'block', cursor: 'crosshair' }}
        >
          <line x1={0} y1={height - 12} x2={WIDTH} y2={height - 12} stroke="var(--gridline)" strokeWidth={1} />
          <path d={area} fill="var(--series-blue-wash)" stroke="none" />
          <path d={path} fill="none" stroke="var(--series-blue)" strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
          {hovered && (
            <>
              <line
                x1={hovered.point.x}
                y1={0}
                x2={hovered.point.x}
                y2={height}
                stroke="var(--baseline)"
                strokeWidth={1}
              />
              <circle
                cx={hovered.point.x}
                cy={hovered.point.y}
                r={4}
                fill="var(--series-blue)"
                stroke="var(--surface-1)"
                strokeWidth={2}
              />
            </>
          )}
        </svg>
      )}
      <div className="muted" style={{ fontSize: 11 }}>
        min {fmt(min)}
      </div>
    </div>
  )
}
