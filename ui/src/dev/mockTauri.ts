/**
 * Dev-only Tauri IPC mock.
 *
 * `main.tsx` installs this automatically when Vite serves the UI outside the
 * desktop shell (no `window.__TAURI_INTERNALS__`). It is loaded through a
 * dynamic import guarded by `import.meta.env.DEV`, so it never ships in a
 * production bundle and never runs inside Tauri.
 *
 * URL flags:
 *   ?mock=connect  start disconnected and show the connect dialog
 *   ?mock=live     connected with live-trading styling
 */

type MockArgs = {
  [key: string]: unknown
  onEvent?: unknown
  req?: unknown
}

type MockHandler = (args: MockArgs) => unknown

interface MockBar {
  time: number
  open: number
  high: number
  low: number
  close: number
  volume: number
}

interface MockEvent {
  event: string
  id: number
  payload: unknown
}

const w = window as unknown as Record<string, unknown>

export function installMockTauri(): void {
  if (w.__TAURI_INTERNALS__) return

  const params = new URLSearchParams(window.location.search)
  const screen = params.get('mock') || 'connected'

  const session = {
    connected: screen !== 'connect',
    key_id: screen === 'live' ? 'AKTE…3456' : 'PKTE…3456',
    base_url:
      screen === 'live'
        ? 'https://api.alpaca.markets'
        : 'https://paper-api.alpaca.markets',
    paper: screen !== 'live',
  }

  const positions = [
    {
      symbol: 'AAPL',
      qty: '120',
      avg_entry_price: '182.40',
      current_price: '231.18',
      market_value: '27741.60',
      unrealized_pl: '5853.60',
      unrealized_plpc: '0.2674',
    },
    {
      symbol: 'NVDA',
      qty: '40',
      avg_entry_price: '121.85',
      current_price: '178.42',
      market_value: '7136.80',
      unrealized_pl: '2262.80',
      unrealized_plpc: '0.4643',
    },
    {
      symbol: 'TSLA',
      qty: '25',
      avg_entry_price: '248.10',
      current_price: '234.66',
      market_value: '5866.50',
      unrealized_pl: '-336.00',
      unrealized_plpc: '-0.0542',
    },
  ]

  const orders = [
    {
      id: '8f1d6c1e-2b7a-4a2e-9d6a-1a2b3c4d5e6f',
      symbol: 'MSFT',
      side: 'buy',
      order_type: 'limit',
      qty: '10',
      filled_qty: '0',
      filled_avg_price: null,
      status: 'new',
      created_at: '2026-11-19T14:31:00Z',
    },
    {
      id: '2c4e6a8b-1d3f-4b5a-8c7d-9e0f1a2b3c4d',
      symbol: 'AAPL',
      side: 'sell',
      order_type: 'stop',
      qty: '20',
      filled_qty: '0',
      filled_avg_price: null,
      status: 'accepted',
      created_at: '2026-11-19T13:58:00Z',
    },
    {
      id: '6a2b4c8d-3e5f-4a7b-9c1d-2e3f4a5b6c7d',
      symbol: 'AMD',
      side: 'buy',
      order_type: 'market',
      qty: '15',
      filled_qty: '15',
      filled_avg_price: '146.22',
      status: 'filled',
      created_at: '2026-11-18T20:04:00Z',
    },
    {
      id: '4d8e2f1a-6b3c-4d5e-8f7a-9b0c1d2e3f4a',
      symbol: 'TSLA',
      side: 'sell',
      order_type: 'limit',
      qty: '5',
      filled_qty: '0',
      filled_avg_price: null,
      status: 'canceled',
      created_at: '2026-11-18T16:12:00Z',
    },
  ]

  const overview = {
    connected: session.connected,
    key_id: session.key_id,
    paper: session.paper,
    base_url: session.base_url,
    account: {
      ok: true,
      data: {
        status: 'ACTIVE',
        currency: 'USD',
        equity: '128450.75',
        buying_power: '86420.10',
        cash: '58200.00',
        portfolio_value: '128450.75',
      },
    },
    clock: {
      ok: true,
      data: {
        is_open: true,
        next_open: '2026-11-20T14:30:00Z',
        next_close: '2026-11-19T21:00:00Z',
      },
    },
    positions: { ok: true, data: positions },
    orders: { ok: true, data: orders },
  }

  // Six months of deterministic daily candles ending at the quoted price.
  const day = 86_400
  const end = Math.floor(Date.now() / 1000 / day) * day
  let close = 181.2
  const bars: MockBar[] = []
  for (let index = 179; index >= 0; index -= 1) {
    const drift = Math.sin(index / 9) * 2.1 + Math.cos(index / 23) * 1.4 + 0.31
    const open = close
    close = Math.max(24, open + drift)
    const high = Math.max(open, close) + 0.6 + (index % 5) * 0.22
    const low = Math.min(open, close) - 0.5 - (index % 3) * 0.19
    bars.push({
      time: end - index * day,
      open: Number(open.toFixed(2)),
      high: Number(high.toFixed(2)),
      low: Number(low.toFixed(2)),
      close: Number(close.toFixed(2)),
      volume: 42_000_000 + ((index * 7919) % 31_000_000),
    })
  }

  // Anchor the series to the position price so the screen is coherent.
  const scale = 231.18 / bars[bars.length - 1].close
  for (const bar of bars) {
    bar.open = Number((bar.open * scale).toFixed(2))
    bar.high = Number((bar.high * scale).toFixed(2))
    bar.low = Number((bar.low * scale).toFixed(2))
    bar.close = Number((bar.close * scale).toFixed(2))
  }

  const last = bars[bars.length - 1]
  const prior = bars[bars.length - 2]
  const change = last.close - prior.close
  const pct = (change / prior.close) * 100

  /** Mask a key the same way the Rust side does. */
  const mask = (key: string): string =>
    key.length > 8 ? `${key.slice(0, 4)}…${key.slice(-4)}` : 'PK…0000'

  const handlers: Record<string, MockHandler> = {
    get_session: () => session,
    connect: (args) => {
      const req = (args.req ?? {}) as {
        keyId?: string
        mode?: 'paper' | 'live'
      }
      session.connected = true
      session.paper = req.mode !== 'live'
      session.base_url = session.paper
        ? 'https://paper-api.alpaca.markets'
        : 'https://api.alpaca.markets'
      session.key_id = mask(req.keyId ?? '')
      overview.connected = session.connected
      overview.paper = session.paper
      overview.base_url = session.base_url
      overview.key_id = session.key_id
      return { ...session }
    },
    disconnect: () => {
      session.connected = false
      overview.connected = false
      return { ...session }
    },
    agent_status: () => ({
      configured: true,
      model: 'inclusionai/ling-3.2-flash:free',
    }),
    /**
     * Fake turn: stream text, then a tool call, then the answer.
     *
     * The channel arrives as the string `__CHANNEL__:<id>`; `invoke` normally
     * builds that from `Channel.toJSON()`, so the id is resolved back to the
     * registered callback the same way Tauri's runtime does.
     */
    agent_chat: (args) => {
      // `?mock=agent-error` exercises the inline failure path without a key.
      if (new URLSearchParams(window.location.search).get('mock') === 'agent-error') {
        return Promise.reject('OpenRouter rate limit hit — retry in a moment.')
      }

      const raw = args.onEvent
      const serialized =
        raw && typeof (raw as { toJSON?: () => unknown }).toJSON === 'function'
          ? String((raw as { toJSON: () => unknown }).toJSON())
          : String(raw)
      const match = /^__CHANNEL__:(\d+)$/.exec(serialized)
      const callback = match
        ? (w[`_${match[1]}`] as ((payload: unknown) => void) | undefined)
        : undefined
      if (!callback) return Promise.resolve()

      let index = 0
      const emit = (message: Record<string, unknown>) =>
        callback({ index: index++, message })

      const script = [
        { wait: 80, event: { type: 'delta', text: 'Let me check that. ' } },
        {
          wait: 260,
          event: {
            type: 'toolStart',
            id: 'call_mock_1',
            name: 'get_quote',
            arguments: '{"symbol":"AAPL"}',
          },
        },
        {
          wait: 420,
          event: {
            type: 'toolDone',
            id: 'call_mock_1',
            name: 'get_quote',
            result:
              '{"symbol":"AAPL","bid":231.1,"ask":231.14,"last":231.12}',
            isError: false,
          },
        },
        { wait: 220, event: { type: 'delta', text: 'AAPL is at ' } },
        { wait: 90, event: { type: 'delta', text: '**$231.12**.' } },
        {
          wait: 60,
          event: {
            type: 'done',
            text: 'Let me check that. AAPL is at **$231.12**.',
            rounds: 2,
          },
        },
      ]

      let at = 0
      for (const step of script) {
        at += step.wait
        window.setTimeout(() => emit(step.event), at)
      }

      return new Promise((resolve) => window.setTimeout(resolve, at))
    },
    get_overview: () => overview,
    get_bars: () => ({
      symbol: 'AAPL',
      bars,
      next_page_token: null,
    }),
    get_quote: () => ({
      symbol: 'AAPL',
      time: new Date().toISOString(),
      bid: Number((last.close - 0.06).toFixed(2)),
      ask: Number((last.close + 0.06).toFixed(2)),
      bid_size: 12,
      ask_size: 9,
      mid: last.close,
    }),
    get_account: () => overview.account.data,
    get_clock: () => overview.clock.data,
    list_positions: () => positions,
    list_orders: () => orders,
    get_asset: () => ({ symbol: 'AAPL', tradable: true }),
    list_assets: () => ({ total: 1, count: 1, assets: [] }),
    create_order: () => orders[0],
    cancel_order: (args) => ({
      canceled: typeof args.id === 'string' ? args.id : orders[0].id,
    }),
    close_position: () => ({ symbol: 'AAPL' }),
    get_ui_state: () => ({
      symbol: 'AAPL',
      range: '6M',
      blotterTab: 'positions',
    }),
    save_ui_state: () => null,
    get_storage_info: () => ({
      path: '~/.local/share/dev.traiding.terminal/storage/traiding.db',
      bytes: 245760,
      bars: 180,
      accounts: 1,
      positions: 3,
      orders: 4,
      quotes: 1,
      backtests: 0,
    }),
    set_stream_symbol: () => null,
    run_backtest: (args) => {
      const req = (args.req ?? {}) as {
        symbol?: string
        days?: number
        fast?: number
        slow?: number
        initialCash?: number
      }
      const fast = req.fast ?? 20
      const slow = req.slow ?? 50
      const initial = req.initialCash ?? 10000
      const points = bars.map((bar, index) => {
        const progress = index / Math.max(1, bars.length - 1)
        const equity =
          initial * (1 + progress * 0.18 + Math.sin(index / 11) * 0.03)
        const benchmark = initial * (bar.close / bars[0].close)
        return {
          time: bar.time,
          equity: Number(equity.toFixed(2)),
          benchmark: Number(benchmark.toFixed(2)),
        }
      })
      const final = points[points.length - 1]
      const closes = bars.slice(-8)
      const closedTrades = closes.slice(0, 5).map((bar, index) => {
        const pnl = index % 3 === 2 ? -42.5 : 118.4 + index * 21
        return {
          entryTime: bar.time - 3 * 86_400,
          exitTime: bar.time,
          entryPrice: Number((bar.close - 4).toFixed(2)),
          exitPrice: bar.close,
          qty: 12.5,
          pnl,
          returnPct: (pnl / (initial / 10)) * 100,
        }
      })
      return {
        symbol: req.symbol ?? 'AAPL',
        strategy: 'SMA crossover',
        fast,
        slow,
        days: req.days ?? 365,
        start: new Date(bars[0].time * 1000).toISOString().slice(0, 10),
        end: new Date(final.time * 1000).toISOString().slice(0, 10),
        initialCash: initial,
        finalEquity: final.equity,
        totalReturnPct: (final.equity / initial - 1) * 100,
        benchmarkReturnPct: (final.benchmark / initial - 1) * 100,
        maxDrawdownPct: -12.4,
        winRatePct: 66.7,
        trades: closedTrades.length,
        wins: 4,
        equityCurve: points,
        closedTrades,
        openPosition: null,
      }
    },
    'plugin:event|listen': (args) => {
      const id = Math.floor(Math.random() * 1_000_000)
      const event = typeof args.event === 'string' ? args.event : ''
      const handlerId =
        typeof args.handler === 'string' || typeof args.handler === 'number'
          ? String(args.handler)
          : ''
      const send = (payload: unknown) => {
        const callback = w[`_${handlerId}`]
        if (typeof callback === 'function') {
          ;(callback as (message: MockEvent) => void)({ event, id, payload })
        }
      }
      window.setTimeout(() => {
        if (event === 'stream:status') {
          send({ channel: 'market', state: 'live' })
          send({ channel: 'orders', state: 'live' })
        }
        if (event === 'market:quote') {
          send({
            symbol: 'AAPL',
            time: new Date().toISOString(),
            bid: Number((last.close - 0.06).toFixed(2)),
            ask: Number((last.close + 0.06).toFixed(2)),
            bidSize: 12,
            askSize: 9,
            mid: last.close,
          })
        }
      }, 120)
      return id
    },
    'plugin:event|unlisten': () => null,
    'plugin:opener|open_url': () => null,
  }

  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: () => {},
  }

  w.__TAURI_INTERNALS__ = {
    invoke: async (command: string, args: MockArgs = {}) => {
      const handler = handlers[command]
      if (!handler) throw `Request failed: ${command}`
      return handler(args)
    },
    transformCallback: (callback: (message: unknown) => void) => {
      const id = Math.floor(Math.random() * 1_000_000)
      w[`_${id}`] = callback
      return id
    },
    // Channel.cleanupCallback calls this once the last message is delivered.
    unregisterCallback: (id: number) => {
      delete w[`_${id}`]
    },
  }

  // Surface the mock in the console so demo data cannot be mistaken for live data.
  console.info(
    `[traiding mock] ${screen}; last close ${last.close} (${change >= 0 ? '+' : ''}${pct.toFixed(2)}%)`,
  )
}
