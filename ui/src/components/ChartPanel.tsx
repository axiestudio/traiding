/**
 * Chart pane: symbol entry, last price, range selector, quote strip and the
 * candlestick chart itself.
 */

import { SegmentedControl } from '@astryxdesign/core/SegmentedControl'
import { SegmentedControlItem } from '@astryxdesign/core/SegmentedControl'
import { TextInput } from '@astryxdesign/core/TextInput'

import { useEffect, useRef, useState } from 'react'

import { fmtNum, signed } from '../api'
import type { Bar, Quote } from '../api'
import { ChartLegend, PriceChart } from './PriceChart'

/** The five ranges the terminal has always offered. */
export const RANGES = [
  { id: '1D', label: '1D', timeframe: '1Min', days: 1 },
  { id: '1M', label: '1M', timeframe: '1Hour', days: 30 },
  { id: '6M', label: '6M', timeframe: '1Day', days: 180 },
  { id: '1Y', label: '1Y', timeframe: '1Day', days: 365 },
  { id: '5Y', label: '5Y', timeframe: '1Day', days: 1825 },
] as const

export type RangeId = (typeof RANGES)[number]['id']

interface ChartPanelProps {
  symbol: string
  range: RangeId
  bars: Bar[]
  quote: Quote | null
  loading: boolean
  error: string | null
  isLive: boolean
  onSymbolChange: (symbol: string) => void
  onRangeChange: (range: RangeId) => void
}

export function ChartPanel({
  symbol,
  range,
  bars,
  quote,
  loading,
  error,
  isLive,
  onSymbolChange,
  onRangeChange,
}: ChartPanelProps) {
  const [draft, setDraft] = useState(symbol)
  const [hovered, setHovered] = useState<Bar | null>(null)
  const inputRef = useRef<HTMLInputElement | null>(null)

  // The symbol can also change from elsewhere (the order ticket), so mirror it
  // into the field — but never while the user is typing in it.
  useEffect(() => {
    if (document.activeElement !== inputRef.current) {
      setDraft(symbol)
    }
  }, [symbol])

  const last = bars.length ? bars[bars.length - 1].close : null
  const { change, pct } = rangeChange(bars)

  function commitSymbol(raw: string) {
    const next = raw.trim().toUpperCase()
    if (/^[A-Z][A-Z0-9.]{0,9}$/.test(next) && next !== symbol) {
      onSymbolChange(next)
    } else {
      setDraft(symbol)
    }
  }

  return (
    <>
      <div className="chart-head">
        <div className="chart-head__symbol">
          <TextInput
            ref={inputRef}
            label="Symbol"
            isLabelHidden
            placeholder="AAPL"
            value={draft}
            onChange={(value) => setDraft(value.toUpperCase())}
            onEnter={() => commitSymbol(draft)}
            onBlur={() => commitSymbol(draft)}
            autoCapitalize="characters"
          />
        </div>

        <div className="chart-head__price">
          <div className="chart-head__last">{last !== null ? fmtNum(last) : '—'}</div>
          {change !== null && pct !== null ? (
            <span className={`cell-num ${change > 0 ? 'up' : change < 0 ? 'down' : 'flat'}`}>
              {signed(change)} ({signed(pct)}%)
            </span>
          ) : null}
        </div>

        <div className="chart-head__ranges">
          <SegmentedControl
            value={range}
            onChange={(value) => onRangeChange(value as RangeId)}
            label="Chart range"
            size="sm"
          >
            {RANGES.map((entry) => (
              <SegmentedControlItem key={entry.id} value={entry.id} label={entry.label} />
            ))}
          </SegmentedControl>
        </div>
      </div>

      <div className="quote-strip">
        <span className="quote-strip__pair quote-strip__pair--bid">
          bid <b>{quote ? fmtNum(quote.bid) : '—'}</b>
        </span>
        <span className="quote-strip__pair quote-strip__pair--ask">
          ask <b>{quote ? fmtNum(quote.ask) : '—'}</b>
        </span>
        <span className="quote-strip__pair">
          mid <b>{quote ? fmtNum(quote.mid) : '—'}</b>
        </span>
        <span className="quote-strip__feed" title="IEX feed">
          <b>IEX</b>
          {isLive ? (
            <span className="quote-strip__live">
              <i aria-hidden="true" />
              live
            </span>
          ) : null}
        </span>
        <span className="quote-strip__time">
          {quote?.time ? new Date(quote.time).toLocaleTimeString() : ''}
        </span>
      </div>

      <div className="chart-body">
        <PriceChart bars={bars} onHover={setHovered} />
        <ChartLegend bar={hovered} symbol={symbol} />
        {loading || error || bars.length === 0 ? (
          <div className="chart-overlay">
            {loading ? `Loading ${symbol}…` : error ? error : `No bars for ${symbol}`}
          </div>
        ) : null}
      </div>
    </>
  )
}

/** Exported for the header's price readout, which needs the same maths. */
export function rangeChange(bars: Bar[]): { change: number | null; pct: number | null } {
  if (bars.length < 2) return { change: null, pct: null }
  const first = bars[0].close
  const last = bars[bars.length - 1].close
  if (first === 0) return { change: null, pct: null }
  const change = last - first
  return { change, pct: (change / first) * 100 }
}
