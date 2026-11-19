/**
 * Application shell.
 *
 * Owns the session, the polling cadence and the symbol/range selection. Every
 * child is presentational: they receive data and callbacks, never reach for the
 * backend themselves, which keeps the polling policy in one place.
 */

import { useCallback, useEffect, useRef, useState } from 'react'
import { SegmentedControl } from '@astryxdesign/core/SegmentedControl'
import { SegmentedControlItem } from '@astryxdesign/core/SegmentedControl'
import { useToast } from '@astryxdesign/core/Toast'

import { api, apiError, onEvent } from './api'
import { AgentPanel } from './agent'
import type {
  Bar,
  OrderUpdateEvent,
  Overview,
  Quote,
  SessionInfo,
  StreamQuote,
  StreamStatus,
  StreamTrade,
} from './api'
import { BacktestPanel } from './components/BacktestPanel'
import { Blotter, type BlotterTab } from './components/Blotter'
import { ChartPanel, RANGES, type RangeId } from './components/ChartPanel'
import { ConnectDialog } from './components/ConnectDialog'
import { OrderTicket } from './components/OrderTicket'
import { TopBar } from './components/TopBar'

const OVERVIEW_INTERVAL_MS = 15_000
const QUOTE_INTERVAL_MS = 20_000

const DISCONNECTED: SessionInfo = { connected: false, paper: true }

type RailTab = 'trade' | 'backtest' | 'agent'
type FeedState = { market: StreamStatus['state']; orders: StreamStatus['state'] }
const FEED_OFF: FeedState = { market: 'offline', orders: 'offline' }
const FEED_CONNECTING: FeedState = { market: 'connecting', orders: 'connecting' }

const BAR_STEP: Record<RangeId, number> = {
  '1D': 60,
  '1M': 3600,
  '6M': 86400,
  '1Y': 86400,
  '5Y': 86400,
}

/** Fold one live IEX trade into the candle for its minute, hour or day. */
function applyTrade(prev: Bar[], trade: StreamTrade, range: RangeId): Bar[] {
  const last = prev[prev.length - 1]
  if (!last) return prev

  const seconds = Math.floor(Date.parse(trade.time) / 1000)
  if (!Number.isFinite(seconds)) return prev

  const step = BAR_STEP[range]
  const bucket = Math.floor(seconds / step) * step
  const lastBucket = Math.floor(last.time / step) * step
  if (bucket < lastBucket) return prev

  if (bucket === lastBucket) {
    return [
      ...prev.slice(0, -1),
      {
        ...last,
        close: trade.price,
        high: Math.max(last.high, trade.price),
        low: Math.min(last.low, trade.price),
        volume: last.volume + trade.size,
      },
    ]
  }

  return [
    ...prev,
    {
      time: bucket,
      open: trade.price,
      high: trade.price,
      low: trade.price,
      close: trade.price,
      volume: trade.size,
    },
  ]
}

export function App() {
  const toast = useToast()

  const [session, setSession] = useState<SessionInfo | null>(null)
  const [overview, setOverview] = useState<Overview | null>(null)
  const [symbol, setSymbol] = useState('AAPL')
  const [range, setRange] = useState<RangeId>('6M')
  const [bars, setBars] = useState<Bar[]>([])
  const [chartVersion, setChartVersion] = useState(0)
  const [quote, setQuote] = useState<Quote | null>(null)
  const [lastTrade, setLastTrade] = useState<{ price: number; time: string } | null>(null)
  const [chartLoading, setChartLoading] = useState(false)
  const [chartError, setChartError] = useState<string | null>(null)
  const [blotterTab, setBlotterTab] = useState<BlotterTab>('positions')
  const [railTab, setRailTab] = useState<RailTab>('trade')
  const [feed, setFeed] = useState<FeedState>(FEED_OFF)
  const [closingSymbol, setClosingSymbol] = useState<string | null>(null)
  const [cancelingId, setCancelingId] = useState<string | null>(null)

  const connected = session?.connected === true
  const feedLive = feed.market === 'live'
  const symbolRef = useRef(symbol)
  const rangeRef = useRef(range)
  const pendingQuoteRef = useRef<StreamQuote | null>(null)
  const pendingTradeRef = useRef<StreamTrade | null>(null)
  const liveTimerRef = useRef<number | null>(null)
  const orderRefreshRef = useRef<number | null>(null)

  // A ref keeps the chart fetch from racing itself when the user types fast.
  const chartRequestRef = useRef(0)
  // Do not write defaults back to disk before the saved values are read.
  const [prefsReady, setPrefsReady] = useState(false)

  // -------------------------------------------------------------------------
  // Derived data
  // -------------------------------------------------------------------------

  const account = overview?.account?.ok ? (overview.account.data ?? null) : null
  const clock = overview?.clock?.ok ? (overview.clock.data ?? null) : null
  const positions = overview?.positions?.ok ? (overview.positions.data ?? []) : []
  const orders = overview?.orders?.ok ? (overview.orders.data ?? []) : []

  // Prefer a live trade, then a two-sided live quote, then the last close. A
  // one-sided after-hours quote must not pull the header to the bid.
  const quotePrice = quote && quote.bid > 0 && quote.ask > 0 ? quote.mid : null
  const lastPrice =
    lastTrade?.price ?? quotePrice ?? (bars.length ? bars[bars.length - 1].close : null)

  // -------------------------------------------------------------------------
  // Loaders
  // -------------------------------------------------------------------------

  const loadOverview = useCallback(async () => {
    try {
      const next = await api.getOverview()
      if (!next.connected) {
        setSession(DISCONNECTED)
        return
      }
      setOverview(next)
    } catch (err) {
      // A transient failure should not blow away a working dashboard.
      console.warn('overview failed:', apiError(err))
    }
  }, [])

  const loadChart = useCallback(async (targetSymbol: string, targetRange: RangeId) => {
    const requestId = ++chartRequestRef.current
    const spec = RANGES.find((entry) => entry.id === targetRange) ?? RANGES[2]

    setChartLoading(true)
    setChartError(null)

    try {
      const series = await api.getBars(targetSymbol, spec.timeframe, spec.days)
      if (requestId !== chartRequestRef.current) return // superseded
      setBars(series.bars)
      // Fit only when a load replaces the series, never on a live tick.
      setChartVersion((version) => version + 1)
    } catch (err) {
      if (requestId !== chartRequestRef.current) return
      setBars([])
      setChartError(apiError(err))
    } finally {
      if (requestId === chartRequestRef.current) setChartLoading(false)
    }
  }, [])

  const loadQuote = useCallback(async (targetSymbol: string) => {
    try {
      const next = await api.getQuote(targetSymbol)
      if (targetSymbol === symbolRef.current) setQuote(next)
    } catch {
      // Quotes are IEX-only and can be missing for thin symbols; blank them.
      if (targetSymbol === symbolRef.current) setQuote(null)
    }
  }, [])

  // -------------------------------------------------------------------------
  // Session
  // -------------------------------------------------------------------------

  useEffect(() => {
    let cancelled = false

    void (async () => {
      const [sessionResult, prefsResult] = await Promise.allSettled([
        api.getSession(),
        api.getUiState(),
      ])
      if (cancelled) return

      if (prefsResult.status === 'fulfilled') {
        const prefs = prefsResult.value
        if (prefs.symbol) setSymbol(prefs.symbol)
        if (prefs.range) setRange(prefs.range as RangeId)
        if (prefs.blotterTab) setBlotterTab(prefs.blotterTab as BlotterTab)
      }
      // Only persist after a successful read; otherwise a missing database
      // would log a rejected write on every change.
      setPrefsReady(prefsResult.status === 'fulfilled')

      if (sessionResult.status === 'fulfilled') {
        const current = sessionResult.value
        setSession(current)
        if (current.connected) {
          setFeed(FEED_CONNECTING)
          void loadOverview()
        } else {
          setFeed(FEED_OFF)
        }
      } else {
        setSession(DISCONNECTED)
      }
    })()

    return () => {
      cancelled = true
    }
  }, [loadOverview])

  // Remember the symbol, range and blotter tab across launches. Debounced so
  // dragging through ranges does not write a row per keystroke.
  useEffect(() => {
    if (!prefsReady) return
    const timer = window.setTimeout(() => {
      void api
        .saveUiState({ symbol, range, blotterTab })
        .catch((err) => console.warn('preferences not saved:', apiError(err)))
    }, 500)
    return () => window.clearTimeout(timer)
  }, [prefsReady, symbol, range, blotterTab])

  function handleConnected(next: SessionInfo) {
    setSession(next)
    setFeed(FEED_CONNECTING)
    toast({
      body: `Connected: ${next.paper ? 'paper' : 'live'}`,
      isAutoHide: true,
    })
    void loadOverview()
  }

  async function handleDisconnect() {
    try {
      await api.disconnect()
    } catch {
      // Disconnecting is best-effort: clear the local view regardless.
    }
    setSession(DISCONNECTED)
    setOverview(null)
    setBars([])
    setQuote(null)
    setLastTrade(null)
    setChartError(null)
    setFeed(FEED_OFF)
  }

  // -------------------------------------------------------------------------
  // Market data
  // -------------------------------------------------------------------------

  useEffect(() => {
    if (!connected) return
    void loadChart(symbol, range)
  }, [connected, symbol, range, loadChart])

  useEffect(() => {
    if (!connected) return
    void loadQuote(symbol)
  }, [connected, symbol, loadQuote])

  useEffect(() => {
    symbolRef.current = symbol
    rangeRef.current = range
    setLastTrade(null)
  }, [symbol, range])

  // A quote belongs to one symbol; never show the previous one while the new
  // REST quote or stream frame is in flight.
  useEffect(() => {
    setQuote(null)
  }, [symbol])

  // Point the Rust feed at the chart's symbol. It is a no-op while detached.
  useEffect(() => {
    if (!connected) return
    void api
      .setStreamSymbol(symbol)
      .catch((err) => console.warn('stream symbol not set:', apiError(err)))
  }, [connected, symbol])

  // Live quote/trade and order events. Quote-driven renders are throttled to
  // 4 Hz so a fast tape cannot starve the canvas or the ticket.
  useEffect(() => {
    if (!connected) return
    let disposed = false
    const unlisteners: Array<() => void> = []

    const flush = () => {
      liveTimerRef.current = null
      const quote = pendingQuoteRef.current
      const trade = pendingTradeRef.current
      pendingQuoteRef.current = null
      pendingTradeRef.current = null
      if (disposed) return

      if (quote && quote.symbol === symbolRef.current) {
        setQuote({
          symbol: quote.symbol,
          time: quote.time,
          bid: quote.bid,
          ask: quote.ask,
          bid_size: quote.bidSize,
          ask_size: quote.askSize,
          mid: quote.mid,
        })
      }

      if (trade && trade.symbol === symbolRef.current) {
        setLastTrade({ price: trade.price, time: trade.time })
        setBars((prev) => applyTrade(prev, trade, rangeRef.current))
      }
    }

    const schedule = () => {
      if (liveTimerRef.current !== null) return
      liveTimerRef.current = window.setTimeout(flush, 250)
    }

    const register = async () => {
      try {
        const listeners = await Promise.all([
          onEvent<StreamQuote>('market:quote', (payload) => {
            pendingQuoteRef.current = payload
            schedule()
          }),
          onEvent<StreamTrade>('market:trade', (payload) => {
            pendingTradeRef.current = payload
            schedule()
          }),
          onEvent<OrderUpdateEvent>('trade:update', (payload) => {
            if (disposed) return
            const copy: Record<string, string> = {
              fill: 'Order filled',
              canceled: 'Order canceled',
              rejected: 'Order rejected',
              expired: 'Order expired',
            }
            if (copy[payload.event]) {
              toast({ body: copy[payload.event], isAutoHide: true })
            }
            if (orderRefreshRef.current !== null) {
              window.clearTimeout(orderRefreshRef.current)
            }
            orderRefreshRef.current = window.setTimeout(() => {
              orderRefreshRef.current = null
              void loadOverview()
            }, 400)
          }),
          onEvent<StreamStatus>('stream:status', (payload) => {
            if (disposed) return
            setFeed((prev) => ({ ...prev, [payload.channel]: payload.state }))
          }),
        ])

        for (const unlisten of listeners) {
          if (disposed) unlisten()
          else unlisteners.push(unlisten)
        }
      } catch (err) {
        console.warn('stream events unavailable:', apiError(err))
      }
    }

    void register()

    return () => {
      disposed = true
      for (const unlisten of unlisteners) unlisten()
      if (liveTimerRef.current !== null) {
        window.clearTimeout(liveTimerRef.current)
        liveTimerRef.current = null
      }
      if (orderRefreshRef.current !== null) {
        window.clearTimeout(orderRefreshRef.current)
        orderRefreshRef.current = null
      }
      pendingQuoteRef.current = null
      pendingTradeRef.current = null
    }
  }, [connected, loadOverview, toast])

  // Events can fire before the listener above is registered; read the state
  // directly so a missed frame cannot leave the badge on "polling".
  useEffect(() => {
    if (!connected) return
    let cancelled = false
    const sync = () => {
      api
        .getFeedStatus()
        .then((next) => {
          if (!cancelled) setFeed(next)
        })
        .catch(() => {})
    }
    void sync()
    const timer = window.setInterval(sync, 3000)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [connected])

  // Poll the account while connected. Chart data refreshes on user action, so
  // the two loops together stay well inside Alpaca's rate limits.
  useEffect(() => {
    if (!connected) return
    const overviewTimer = setInterval(() => void loadOverview(), OVERVIEW_INTERVAL_MS)
    // REST quotes on Basic are at least 15 minutes old. Once the IEX socket is
    // live it is the only quote source; polling resumes if the socket drops.
    const quoteTimer =
      feed.market === 'live' ? null : setInterval(() => void loadQuote(symbol), QUOTE_INTERVAL_MS)
    return () => {
      clearInterval(overviewTimer)
      if (quoteTimer !== null) clearInterval(quoteTimer)
    }
  }, [connected, symbol, feed.market, loadOverview, loadQuote])

  // -------------------------------------------------------------------------
  // Actions
  // -------------------------------------------------------------------------

  async function closePosition(targetSymbol: string) {
    setClosingSymbol(targetSymbol)
    try {
      await api.closePosition(targetSymbol)
      toast({ body: 'Close order sent', isAutoHide: true })
      await loadOverview()
    } catch (err) {
      toast({ body: apiError(err), type: 'error' })
    } finally {
      setClosingSymbol(null)
    }
  }

  async function cancelOrder(id: string) {
    setCancelingId(id)
    try {
      await api.cancelOrder(id)
      toast({ body: 'Order canceled', isAutoHide: true })
      await loadOverview()
    } catch (err) {
      toast({ body: apiError(err), type: 'error' })
    } finally {
      setCancelingId(null)
    }
  }

  function orderSubmitted() {
    toast({ body: 'Order submitted', isAutoHide: true })
    void loadOverview()
  }

  // Ctrl/Cmd+K focuses the symbol field, like a terminal.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        const picker = document.querySelector<HTMLButtonElement>('[data-symbol-picker]')
        picker?.focus()
        picker?.click()
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  // -------------------------------------------------------------------------
  // Render
  // -------------------------------------------------------------------------

  if (session === null) {
    return <div className="app-shell" />
  }

  const activeSession = session.connected ? session : DISCONNECTED

  return (
    <div className="app-shell">
      <TopBar
        session={activeSession}
        account={account}
        clock={clock}
        feedLive={feedLive}
        onDisconnect={handleDisconnect}
      />

      <main className="terminal">
        <section className="pane pane--chart">
          <ChartPanel
            symbol={symbol}
            range={range}
            bars={bars}
            quote={quote}
            price={lastPrice}
            resetKey={String(chartVersion)}
            loading={chartLoading}
            error={chartError}
            isLive={feed.market === 'live'}
            onSymbolChange={setSymbol}
            onRangeChange={setRange}
          />
        </section>

        <section className="pane pane--blotter">
          <Blotter
            tab={blotterTab}
            onTabChange={setBlotterTab}
            positions={positions}
            orders={orders}
            onClosePosition={closePosition}
            onCancelOrder={cancelOrder}
            closingSymbol={closingSymbol}
            cancelingId={cancelingId}
          />
        </section>

        <aside className="pane pane--ticket rail">
          <div className="rail__tabs">
            <SegmentedControl
              value={railTab}
              onChange={(value) =>
                setRailTab(
                  value === 'backtest' ? 'backtest' : value === 'agent' ? 'agent' : 'trade',
                )
              }
              label="Right rail"
              size="sm"
              layout="fill"
            >
              <SegmentedControlItem value="trade" label="Trade" />
              <SegmentedControlItem value="backtest" label="Backtest" />
              <SegmentedControlItem value="agent" label="Agent" />
            </SegmentedControl>
          </div>

          <div className="rail__body">
            {/* Both stay mounted so switching tabs does not discard ticket or
                backtest state. */}
            <div className={railTab === 'trade' ? undefined : 'rail__hide'}>
              <OrderTicket
                symbol={symbol}
                account={account}
                positions={positions}
                lastPrice={lastPrice}
                onSubmitted={orderSubmitted}
              />
            </div>
            <div className={railTab === 'backtest' ? undefined : 'rail__hide'}>
              <BacktestPanel symbol={symbol} />
            </div>
            <div
              className={
                railTab === 'agent' ? 'rail__pane' : 'rail__hide'
              }
            >
              <AgentPanel />
            </div>
          </div>
        </aside>
      </main>

      <ConnectDialog isOpen={!session.connected} onConnected={handleConnected} />
    </div>
  )
}
