/** Backtest equity curve: strategy versus buy-and-hold. */

import { useEffect, useRef } from 'react'
import {
  ColorType,
  LineSeries,
  LineStyle,
  createChart,
  type IChartApi,
  type ISeriesApi,
  type UTCTimestamp,
} from 'lightweight-charts'

import type { EquityPoint } from '../api'

const UP = '#22c55e'
const BENCH = '#8b93a3'
const GRID = 'rgba(255, 255, 255, 0.05)'
const AXIS_TEXT = '#8b93a3'

function money(value: number): string {
  if (Math.abs(value) >= 10_000) return `$${Math.round(value / 1000)}k`
  return `$${Math.round(value)}`
}

interface EquityChartProps {
  points: EquityPoint[]
}

export function EquityChart({ points }: EquityChartProps) {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const chartRef = useRef<IChartApi | null>(null)
  const strategyRef = useRef<ISeriesApi<'Line'> | null>(null)
  const benchmarkRef = useRef<ISeriesApi<'Line'> | null>(null)

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const chart = createChart(container, {
      autoSize: true,
      layout: {
        background: { type: ColorType.Solid, color: 'transparent' },
        textColor: AXIS_TEXT,
        fontFamily: 'ui-monospace, "JetBrains Mono", Menlo, Consolas, monospace',
        fontSize: 10,
        attributionLogo: false,
      },
      grid: {
        vertLines: { color: GRID },
        horzLines: { color: GRID },
      },
      rightPriceScale: {
        borderColor: 'rgba(255, 255, 255, 0.08)',
        scaleMargins: { top: 0.12, bottom: 0.08 },
      },
      timeScale: {
        borderColor: 'rgba(255, 255, 255, 0.08)',
        rightOffset: 1,
        fixLeftEdge: true,
      },
      crosshair: {
        vertLine: { color: 'rgba(139, 147, 163, 0.4)', labelBackgroundColor: '#232935' },
        horzLine: { color: 'rgba(139, 147, 163, 0.4)', labelBackgroundColor: '#232935' },
      },
      handleScroll: { mouseWheel: false, pressedMouseMove: false },
      handleScale: { mouseWheel: false, pinch: false },
    })

    const strategy = chart.addSeries(LineSeries, {
      color: UP,
      lineWidth: 2,
      priceLineVisible: false,
      lastValueVisible: false,
      priceFormat: { type: 'custom', formatter: money },
    })
    const benchmark = chart.addSeries(LineSeries, {
      color: BENCH,
      lineWidth: 1,
      lineStyle: LineStyle.Dashed,
      priceLineVisible: false,
      lastValueVisible: false,
      priceFormat: { type: 'custom', formatter: money },
    })

    chartRef.current = chart
    strategyRef.current = strategy
    benchmarkRef.current = benchmark

    return () => {
      chart.remove()
      chartRef.current = null
      strategyRef.current = null
      benchmarkRef.current = null
    }
  }, [])

  useEffect(() => {
    const strategy = strategyRef.current
    const benchmark = benchmarkRef.current
    const chart = chartRef.current
    if (!strategy || !benchmark || !chart) return

    strategy.setData(
      points.map((point) => ({
        time: point.time as UTCTimestamp,
        value: point.equity,
      })),
    )
    benchmark.setData(
      points.map((point) => ({
        time: point.time as UTCTimestamp,
        value: point.benchmark,
      })),
    )

    if (points.length > 0) chart.timeScale().fitContent()
  }, [points])

  return <div ref={containerRef} className="equity-chart" />
}
