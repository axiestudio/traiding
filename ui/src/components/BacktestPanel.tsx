/** Backtest form, metrics and equity curve for the selected symbol. */

import { useEffect, useState } from 'react'
import { Button } from '@astryxdesign/core/Button'
import { NumberInput } from '@astryxdesign/core/NumberInput'
import { Selector } from '@astryxdesign/core/Selector'

import { api, apiError, fmtMoney, fmtNum, fmtQty, signed } from '../api'
import type { BacktestResult } from '../api'
import { EquityChart } from './EquityChart'

interface BacktestPanelProps {
  symbol: string
}

const RANGES = [
  { value: '365', label: '1Y' },
  { value: '730', label: '2Y' },
  { value: '1825', label: '5Y' },
]

function pct(value: number): string {
  return `${signed(value)}%`
}

function tone(value: number): string {
  return value > 0 ? 'up' : value < 0 ? 'down' : 'flat'
}

function shortDate(time: number): string {
  return new Date(time * 1000).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
  })
}

export function BacktestPanel({ symbol }: BacktestPanelProps) {
  const [range, setRange] = useState('365')
  const [fast, setFast] = useState<number | null>(20)
  const [slow, setSlow] = useState<number | null>(50)
  const [cash, setCash] = useState<number | null>(10_000)
  const [running, setRunning] = useState(false)
  const [error, setError] = useState('')
  const [result, setResult] = useState<BacktestResult | null>(null)

  // A result belongs to the symbol it was run on; never leave AAPL numbers
  // sitting under a MSFT chart.
  useEffect(() => {
    setResult(null)
    setError('')
  }, [symbol])

  async function run() {
    setError('')
    const fastWindow = fast === null ? 0 : Math.round(fast)
    const slowWindow = slow === null ? 0 : Math.round(slow)
    if (fastWindow < 2 || slowWindow <= fastWindow) {
      setError('Fast must be below slow.')
      return
    }
    if (!cash || cash <= 0) {
      setError('Enter starting cash.')
      return
    }

    setRunning(true)
    try {
      setResult(
        await api.runBacktest({
          symbol,
          days: Number(range),
          fast: fastWindow,
          slow: slowWindow,
          initialCash: cash,
          feeBps: 0,
        }),
      )
    } catch (err) {
      setError(apiError(err))
    } finally {
      setRunning(false)
    }
  }

  return (
    <div className="backtest">
      <div className="backtest__heading">
        <span>Backtest</span>
        <span className="backtest__symbol">{symbol}</span>
      </div>

      <div className="backtest__strategy">
        <span>SMA crossover</span>
        <Selector
          label="Range"
          options={RANGES}
          value={range}
          onChange={setRange}
          isDisabled={running}
        />
      </div>

      <div className="ticket__row">
        <NumberInput
          label="Fast"
          value={fast}
          onChange={setFast}
          min={2}
          step={1}
          isDisabled={running}
        />
        <NumberInput
          label="Slow"
          value={slow}
          onChange={setSlow}
          min={3}
          step={1}
          isDisabled={running}
        />
      </div>

      <NumberInput
        label="Starting cash"
        value={cash}
        onChange={setCash}
        min={1}
        step={1000}
        isDisabled={running}
        units="$"
      />

      <Button
        label={running ? 'Running…' : 'Run backtest'}
        variant="primary"
        onClick={run}
        isLoading={running}
        isDisabled={running || !symbol}
        width="100%"
      />

      {error ? <div className="ticket__error">{error}</div> : null}

      {result ? (
        <>
          <div className="backtest__meta">
            {result.start} – {result.end} · {result.trades} trades
            {result.openPosition ? ' · open' : ''}
          </div>

          <div className="backtest__metrics">
            <Metric label="Return" value={pct(result.totalReturnPct)} tone={tone(result.totalReturnPct)} />
            <Metric
              label="Benchmark"
              value={pct(result.benchmarkReturnPct)}
              tone={tone(result.benchmarkReturnPct)}
            />
            <Metric
              label="Max drawdown"
              value={pct(result.maxDrawdownPct)}
              tone="down"
            />
            <Metric
              label="Win rate"
              value={`${fmtNum(result.winRatePct)}%`}
              tone={result.winRatePct >= 50 ? 'up' : 'flat'}
            />
            <Metric label="Trades" value={`${result.wins}/${result.trades}`} tone="flat" />
            <Metric label="Equity" value={fmtMoney(result.finalEquity)} tone="flat" />
          </div>

          <div className="backtest__chart-head">
            <span className="backtest__legend backtest__legend--strategy">Strategy</span>
            <span className="backtest__legend backtest__legend--bench">Buy &amp; hold</span>
          </div>
          <EquityChart points={result.equityCurve} />

          {result.openPosition ? (
            <div className="backtest__open">
              Open {fmtQty(result.openPosition.qty)} @ {fmtMoney(result.openPosition.entryPrice)}
            </div>
          ) : null}

          {result.closedTrades.length > 0 ? (
            <div className="backtest__trades">
              {result.closedTrades
                .slice(-4)
                .reverse()
                .map((trade, index) => (
                  <div className="backtest__trade" key={`${trade.entryTime}-${index}`}>
                    <span>
                      {shortDate(trade.entryTime)} → {shortDate(trade.exitTime)}
                    </span>
                    <span className={tone(trade.pnl)}>{fmtMoney(trade.pnl)}</span>
                  </div>
                ))}
            </div>
          ) : null}
        </>
      ) : (
        <div className="backtest__empty">No runs yet</div>
      )}
    </div>
  )
}

function Metric({ label, value, tone }: { label: string; value: string; tone: string }) {
  return (
    <div className="backtest__metric">
      <span>{label}</span>
      <span className={tone}>{value}</span>
    </div>
  )
}
