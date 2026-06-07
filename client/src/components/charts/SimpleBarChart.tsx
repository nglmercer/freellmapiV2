import { useState } from 'react'

export interface SimpleBarChartProps {
  data: Record<string, unknown>[]
  dataKey: string
  xKey: string
  height?: number
  color?: string
  unit?: string
  name?: string
}

export function SimpleBarChart({
  data,
  dataKey,
  xKey,
  height = 240,
  color = 'var(--foreground)',
  unit = '',
  name,
}: SimpleBarChartProps) {
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null)

  if (!data.length) return null

  const getValue = (item: Record<string, unknown>, key: string): number => {
    const val = item[key]
    return typeof val === 'number' ? val : Number(val) || 0
  }

  const getLabel = (item: Record<string, unknown>, key: string): string => {
    const val = item[key]
    return val != null ? String(val) : ''
  }

  const maxValue = Math.max(...data.map(d => getValue(d, dataKey)))
  const padding = { top: 20, right: 20, bottom: 40, left: 50 }
  const chartWidth = 600
  const chartHeight = height
  const innerWidth = chartWidth - padding.left - padding.right
  const innerHeight = chartHeight - padding.top - padding.bottom

  const barWidth = (innerWidth / data.length) * 0.7
  const barGap = (innerWidth / data.length) * 0.3

  // Generate Y-axis ticks
  const yTicks = 5
  const yStep = maxValue / yTicks
  const yTickValues = Array.from({ length: yTicks + 1 }, (_, i) => Math.round(i * yStep))

  return (
    <svg
      viewBox={`0 0 ${chartWidth} ${chartHeight}`}
      className="w-full"
      style={{ height }}
    >
      {/* Grid lines */}
      {yTickValues.map((value, i) => {
        const y = padding.top + innerHeight - (value / maxValue) * innerHeight
        return (
          <g key={i}>
            <line
              x1={padding.left}
              y1={y}
              x2={chartWidth - padding.right}
              y2={y}
              stroke="var(--border)"
              strokeDasharray="2 4"
            />
            <text
              x={padding.left - 8}
              y={y + 4}
              textAnchor="end"
              fontSize="11"
              fill="var(--muted-foreground)"
            >
              {value}
              {unit}
            </text>
          </g>
        )
      })}

      {/* Bars */}
      {data.map((item, i) => {
        const value = getValue(item, dataKey)
        const barHeight = (value / maxValue) * innerHeight
        const x = padding.left + i * (barWidth + barGap) + barGap / 2
        const y = padding.top + innerHeight - barHeight

        return (
          <g
            key={i}
            onMouseEnter={() => setHoveredIndex(i)}
            onMouseLeave={() => setHoveredIndex(null)}
          >
            <rect
              x={x}
              y={y}
              width={barWidth}
              height={barHeight}
              fill={color}
              opacity={hoveredIndex === null || hoveredIndex === i ? 1 : 0.6}
              style={{ cursor: 'pointer', transition: 'opacity 0.2s' }}
            />
            {/* X-axis label */}
            <text
              x={x + barWidth / 2}
              y={chartHeight - padding.bottom + 20}
              textAnchor="middle"
              fontSize="11"
              fill="var(--muted-foreground)"
            >
              {getLabel(item, xKey)}
            </text>
            {/* Tooltip on hover */}
            {hoveredIndex === i && (
              <g>
                <rect
                  x={x + barWidth / 2 - 40}
                  y={y - 30}
                  width="80"
                  height="24"
                  fill="var(--popover)"
                  stroke="var(--border)"
                  rx="4"
                />
                <text
                  x={x + barWidth / 2}
                  y={y - 14}
                  textAnchor="middle"
                  fontSize="12"
                  fill="var(--popover-foreground)"
                >
                  {name || dataKey}: {value}
                  {unit}
                </text>
              </g>
            )}
          </g>
        )
      })}

      {/* X-axis line */}
      <line
        x1={padding.left}
        y1={chartHeight - padding.bottom}
        x2={chartWidth - padding.right}
        y2={chartHeight - padding.bottom}
        stroke="var(--border)"
      />
    </svg>
  )
}
