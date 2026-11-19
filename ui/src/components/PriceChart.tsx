/**
 * Lightweight Charts wrapper.
 *
 * The library is canvas-based and framework-agnostic, so this component owns
 * exactly one job: create the chart once, keep it sized to its container, and
 * push new bars in. Lightweight Charts has no React binding of its own, which
 * is why the lifecycle lives here rather than in a dependency.
 */

import { useEffect, useRef } from 'react'
import {
  CandlestickSeries,
  ColorType,
  CrosshairMode,
  HistogramSeries,
  createChart,
  type CandlestickData,
  type IChartApi,
  type ISeriesApi,
  type MouseEventParams,
  type Time,
  type UTCTimestamp,
} from 'lightweight-charts'

import type { Bar } from '../api'
import { fmtNum } from '../api'

interface PriceChartProps {
  bars: Bar[]
  onHover?: (bar: Bar | null) => void
}

const UP = '#22c55e'
const DOWN = '#ef4444'
const GRID = 'rgba(255, 255, 255, 0.05)'
const AXIS_TEXT = '#8b93a3'

/**
 * Volume histogram: a separate pane below the price, tinted by candle colour.
 * A new array is built each time because the plugin mutates the colour field.
 */
function volumeData(bars: Bar[]): { time: UTCTimestamp; value: number; color: string }[] {
  return bars.map((bar) => ({
    time: bar.time as UTCTimestamp,
    value: bar.volume,
    color: bar.close >= bar.open ? 'rgba(34, 197, 94, 0.32)' : 'rgba(239, 68, 68, 0.32)',
  }))
}

function candleData(bars: Bar[]): CandlestickData<Time>[] {
  return bars.map((bar) => ({
    time: bar.time as UTCTimestamp,
    open: bar.open,
    high: bar.high,
    low: bar.low,
    close: bar.close,
  }))
}

export function PriceChart({ bars, onHover }: PriceChartProps) {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const chartRef = useRef<IChartApi | null>(null)
  const candleRef = useRef<ISeriesApi<'Candlestick'> | null>(null)
  const volumeRef = useRef<ISeriesApi<'Histogram'> | null>(null)
  const barsRef = useRef<Bar[]>([])
  const signatureRef = useRef('')
  const hoverRef = useRef<PriceChartProps['onHover']>(onHover)

  // Keep the callback fresh without tearing the chart down every render.
  useEffect(() => {
    hoverRef.current = onHover
  }, [onHover])

  // Create the chart once; the dependency list is intentionally empty.
  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const chart = createChart(container, {
      autoSize: true,
      layout: {
        background: { type: ColorType.Solid, color: 'transparent' },
        textColor: AXIS_TEXT,
        fontFamily: 'ui-monospace, "JetBrains Mono", Menlo, Consolas, monospace',
        fontSize: 11,
        attributionLogo: false,
      },
      grid: {
        vertLines: { color: GRID },
        horzLines: { color: GRID },
      },
      rightPriceScale: {
        borderColor: 'rgba(255, 255, 255, 0.08)',
        scaleMargins: { top: 0.08, bottom: 0.06 },
      },
      timeScale: {
        borderColor: 'rgba(255, 255, 255, 0.08)',
        rightOffset: 4,
        fixLeftEdge: true,
        lockVisibleTimeRangeOnResize: true,
      },
      crosshair: {
        mode: CrosshairMode.Normal,
        vertLine: {
          color: 'rgba(139, 147, 163, 0.5)',
          labelBackgroundColor: '#232935',
        },
        horzLine: {
          color: 'rgba(139, 147, 163, 0.5)',
          labelBackgroundColor: '#232935',
        },
      },
      handleScroll: { mouseWheel: true, pressedMouseMove: true },
      handleScale: { mouseWheel: true, pinch: true },
    })

    const candles = chart.addSeries(CandlestickSeries, {
      upColor: UP,
      downColor: DOWN,
      borderUpColor: UP,
      borderDownColor: DOWN,
      wickUpColor: UP,
      wickDownColor: DOWN,
      priceLineVisible: false,
      lastValueVisible: true,
    })

    // Volume goes in its own thin pane. Its height is a stretch factor, so
    // autoSize resets it whenever the window changes; re-apply it after every
    // resize instead of only once at mount.
    const volume = chart.addSeries(
      HistogramSeries,
      {
        priceFormat: { type: 'volume' },
        priceLineVisible: false,
        lastValueVisible: false,
      },
      1,
    )
    const fitResize = () => {
      const pane = chart.panes()[1]
      if (pane && pane.getHeight() !== 72) pane.setHeight(72)
      // fitContent ran once at data load, possibly before the pane had its final
      // width; re-fit after a resize or the series ends mid-chart.
      if (barsRef.current.length > 0) chart.timeScale().fitContent()
    }
    const volumeFrame = requestAnimationFrame(fitResize)
    let resizeFrame = 0
    const sizeObserver = new ResizeObserver(() => {
      cancelAnimationFrame(resizeFrame)
      resizeFrame = requestAnimationFrame(fitResize)
    })
    sizeObserver.observe(container)

    chart.subscribeCrosshairMove((param: MouseEventParams<Time>) => {
      const handler = hoverRef.current
      if (!handler) return
      if (param.time === undefined || !param.point) {
        handler(null)
        return
      }
      const time = param.time as number
      handler(barsRef.current.find((bar) => bar.time === time) ?? null)
    })

    chartRef.current = chart
    candleRef.current = candles
    volumeRef.current = volume

    return () => {
      sizeObserver.disconnect()
      cancelAnimationFrame(volumeFrame)
      cancelAnimationFrame(resizeFrame)
      chart.remove()
      chartRef.current = null
      candleRef.current = null
      volumeRef.current = null
    }
  }, [])

  // Push data whenever the series changes, then fit the visible range.
  useEffect(() => {
    barsRef.current = bars

    const candles = candleRef.current
    const volume = volumeRef.current
    const chart = chartRef.current
    if (!candles || !volume || !chart) return

    candles.setData(candleData(bars))
    volume.setData(volumeData(bars))

    // Live ticks mutate the last bar in place. Re-fitting on those would drag
    // the visible range under the cursor on every trade, so only fit when the
    // bar window itself changed (new symbol or range).
    const signature = bars.length
      ? `${bars[0].time}:${bars[bars.length - 1].time}:${bars.length}`
      : ''
    if (signature !== signatureRef.current) {
      signatureRef.current = signature
      if (bars.length > 0) chart.timeScale().fitContent()
    }
  }, [bars])

  return <div ref={containerRef} className="chart-canvas" />
}

/** The legend lives in HTML so it can use the app's type scale. */
export function ChartLegend({ bar, symbol }: { bar: Bar | null; symbol: string }) {
  if (!bar) {
    return (
      <div className="chart-legend chart-legend--idle">
        <span className="chart-legend__symbol">{symbol}</span>
        <span className="chart-legend__hint">hover for OHLCV</span>
      </div>
    )
  }

  const change = bar.close - bar.open
  const pct = bar.open ? (change / bar.open) * 100 : 0
  const tone = change > 0 ? 'up' : change < 0 ? 'down' : 'flat'

  return (
    <div className="chart-legend">
      <span className="chart-legend__symbol">{symbol}</span>
      <span className="chart-legend__pair">
        <em>O</em> {fmtNum(bar.open)}
      </span>
      <span className="chart-legend__pair">
        <em>H</em> {fmtNum(bar.high)}
      </span>
      <span className="chart-legend__pair">
        <em>L</em> {fmtNum(bar.low)}
      </span>
      <span className="chart-legend__pair">
        <em>C</em> {fmtNum(bar.close)}
      </span>
      <span className={`chart-legend__pair chart-legend__change ${tone}`}>
        {pct >= 0 ? '+' : '−'}
        {fmtNum(Math.abs(pct))}%
      </span>
      <span className="chart-legend__pair">
        <em>V</em> {fmtNum(bar.volume, 0)}
      </span>
    </div>
  )
}
