import { useState } from 'react'

interface Line {
  dataKey: string
  name: string
  color: string
}

interface SimpleLineChartProps {
  data: Record<string, unknown>[]
  lines: Line[]
  xKey: string
  height?: number
}

export function SimpleLineChart({
  data,
  lines,
  xKey,
  height = 240,
}: SimpleLineChartProps) {
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

  const padding = { top: 20, right: 20, bottom: 40, left: 50 }
  const chartWidth = 600
  const chartHeight = height
  const innerWidth = chartWidth - padding.left - padding.right
  const innerHeight = chartHeight - padding.top - padding.bottom

  // Find max value across all lines
  const maxValue = Math.max(
    ...data.flatMap(d => lines.map(line => getValue(d, line.dataKey)))
  )

  // Generate Y-axis ticks
  const yTicks = 5
  const yStep = maxValue / yTicks
  const yTickValues = Array.from({ length: yTicks + 1 }, (_, i) => Math.round(i * yStep))

  // Helper to get point coordinates
  const getPoint = (index: number, value: number) => {
    const x = padding.left + (index / (data.length - 1)) * innerWidth
    const y = padding.top + innerHeight - (value / maxValue) * innerHeight
    return { x, y }
  }

  // Generate path for a line
  const generatePath = (dataKey: string) => {
    return data
      .map((item, i) => {
        const point = getPoint(i, getValue(item, dataKey))
        return `${i === 0 ? 'M' : 'L'} ${point.x} ${point.y}`
      })
      .join(' ')
  }

  // Format x-axis labels (show first, middle, last)
  const xLabelIndices = [0, Math.floor(data.length / 2), data.length - 1]

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
            </text>
          </g>
        )
      })}

      {/* Lines */}
      {lines.map((line, lineIndex) => (
        <path
          key={lineIndex}
          d={generatePath(line.dataKey)}
          fill="none"
          stroke={line.color}
          strokeWidth="2"
        />
      ))}

      {/* Data points and hover areas */}
      {data.map((item, i) => {
        const x = padding.left + (i / (data.length - 1)) * innerWidth
        return (
          <g key={i}>
            {/* Invisible hover area */}
            <rect
              x={x - innerWidth / data.length / 2}
              y={padding.top}
              width={innerWidth / data.length}
              height={innerHeight}
              fill="transparent"
              onMouseEnter={() => setHoveredIndex(i)}
              onMouseLeave={() => setHoveredIndex(null)}
              style={{ cursor: 'pointer' }}
            />

            {/* Vertical line on hover */}
            {hoveredIndex === i && (
              <line
                x1={x}
                y1={padding.top}
                x2={x}
                y2={padding.top + innerHeight}
                stroke="var(--border)"
                strokeDasharray="2 2"
              />
            )}

            {/* Data points for each line */}
            {lines.map((line, lineIndex) => {
              const point = getPoint(i, getValue(item, line.dataKey))
              return (
                <circle
                  key={lineIndex}
                  cx={point.x}
                  cy={point.y}
                  r={hoveredIndex === i ? 5 : 3}
                  fill={line.color}
                  style={{ transition: 'r 0.2s' }}
                />
              )
            })}

            {/* Tooltip on hover */}
            {hoveredIndex === i && (
              <g>
                <rect
                  x={x - 60}
                  y={padding.top - 10}
                  width="120"
                  height={lines.length * 20 + 10}
                  fill="var(--popover)"
                  stroke="var(--border)"
                  rx="4"
                />
                <text
                  x={x}
                  y={padding.top + 6}
                  textAnchor="middle"
                  fontSize="11"
                  fill="var(--muted-foreground)"
                >
                  {getLabel(item, xKey)}
                </text>
                {lines.map((line, lineIndex) => (
                  <text
                    key={lineIndex}
                    x={x}
                    y={padding.top + 22 + lineIndex * 20}
                    textAnchor="middle"
                    fontSize="12"
                    fill={line.color}
                  >
                    {line.name}: {getValue(item, line.dataKey)}
                  </text>
                ))}
              </g>
            )}

            {/* X-axis labels */}
            {xLabelIndices.includes(i) && (
              <text
                x={x}
                y={chartHeight - padding.bottom + 20}
                textAnchor="middle"
                fontSize="11"
                fill="var(--muted-foreground)"
              >
                {getLabel(item, xKey)}
              </text>
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
