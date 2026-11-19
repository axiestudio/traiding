/**
 * Typed wrappers around the Rust IPC commands.
 *
 * Every command is `Result<T, String>` on the Rust side, so a rejected promise
 * carries the already-clean message produced by `src/error.rs`. Nothing in this
 * file talks to Alpaca directly and no credentials are ever stored here.
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ---------------------------------------------------------------------------
// Wire types — shaped by src/commands.rs
// ---------------------------------------------------------------------------

export interface SessionInfo {
  connected: boolean
  key_id?: string
  base_url?: string
  paper: boolean
}

export interface Bar {
  /** Unix seconds. */
  time: number
  open: number
  high: number
  low: number
  close: number
  volume: number
}

export interface BarSeries {
  symbol: string
  bars: Bar[]
  next_page_token: string | null
}

export interface Quote {
  symbol: string
  time: string
  bid: number
  ask: number
  bid_size: number
  ask_size: number
  mid: number
}

export interface Account {
  id?: string
  status?: string
  currency?: string
  buying_power?: string
  regt_buying_power?: string
  cash?: string
  portfolio_value?: string
  equity?: string
  last_equity?: string
  long_market_value?: string
  short_market_value?: string
  pattern_day_trader?: boolean
  trading_blocked?: boolean
  account_blocked?: boolean
}

export interface Clock {
  timestamp?: string
  is_open?: boolean
  next_open?: string
  next_close?: string
}

export interface Position {
  asset_id?: string
  symbol: string
  exchange?: string
  asset_class?: string
  qty?: string
  quantity?: string
  avg_entry_price?: string
  average_entry_price?: string
  side?: string
  market_value?: string
  cost_basis?: string
  current_price?: string
  lastday_price?: string
  change_today?: string
  unrealized_pl?: string
  unrealized_gain_total?: string
  unrealized_plpc?: string
  unrealized_gain_total_percent?: string
  /** Astryx's Table requires `Record<string, unknown>` rows. */
  [key: string]: unknown
}

export interface Order {
  id: string
  client_order_id?: string
  created_at?: string
  updated_at?: string
  submitted_at?: string
  filled_at?: string | null
  asset_id?: string
  symbol: string
  exchange?: string
  asset_class?: string
  qty?: string
  filled_qty?: string
  filled_avg_price?: string | null
  order_type?: string
  type?: string
  side?: string
  time_in_force?: string
  limit_price?: string | null
  stop_price?: string | null
  status?: string
  extended_hours?: boolean
  /** Astryx's Table requires `Record<string, unknown>` rows. */
  [key: string]: unknown
}

export interface Section<T> {
  ok: boolean
  data?: T
  error?: string
}

export interface Overview {
  connected: boolean
  key_id?: string
  paper?: boolean
  base_url?: string
  account?: Section<Account>
  clock?: Section<Clock>
  positions?: Section<Position[]>
  orders?: Section<Order[]>
}

export interface AssetRow {
  symbol: string
  exchange: string
  class: string
  tradable: boolean
  fractionable: boolean
  shortable: boolean
  easy_to_borrow: boolean
}

export interface AssetsPage {
  total: number
  count: number
  assets: AssetRow[]
}

export interface UiState {
  symbol: string
  range: string
  blotterTab: string
}

export interface StorageStats {
  path: string
  bytes: number
  bars: number
  accounts: number
  positions: number
  orders: number
  quotes: number
  backtests: number
}

export interface StreamQuote {
  symbol: string
  time: string
  bid: number
  ask: number
  bidSize: number
  askSize: number
  mid: number
}

export interface StreamTrade {
  symbol: string
  time: string
  price: number
  size: number
}

export interface StreamStatus {
  channel: 'market' | 'orders'
  state: 'connecting' | 'live' | 'offline'
}

export interface FeedStatus {
  market: StreamStatus['state']
  orders: StreamStatus['state']
}

export interface OrderUpdateEvent {
  event: string
  order: Order
}

export interface BacktestRequest {
  symbol: string
  days: number
  fast: number
  slow: number
  initialCash: number
  feeBps: number
}

export interface EquityPoint {
  time: number
  equity: number
  benchmark: number
}

export interface ClosedTrade {
  entryTime: number
  exitTime: number
  entryPrice: number
  exitPrice: number
  qty: number
  pnl: number
  returnPct: number
}

export interface OpenPosition {
  entryTime: number
  entryPrice: number
  qty: number
  currentPrice: number
  unrealizedPnl: number
}

export interface BacktestResult {
  symbol: string
  strategy: string
  fast: number
  slow: number
  days: number
  start: string
  end: string
  initialCash: number
  finalEquity: number
  totalReturnPct: number
  benchmarkReturnPct: number
  maxDrawdownPct: number
  winRatePct: number
  trades: number
  wins: number
  equityCurve: EquityPoint[]
  closedTrades: ClosedTrade[]
  openPosition: OpenPosition | null
}

export interface CreateOrderRequest {
  symbol: string
  side: 'buy' | 'sell'
  type: 'market' | 'limit' | 'stop' | 'stop_limit'
  qty?: number
  notional?: number
  limitPrice?: number
  stopPrice?: number
  timeInForce?: 'day' | 'gtc' | 'ioc' | 'fok'
  extendedHours?: boolean
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

export const api = {
  getSession: () => invoke<SessionInfo>('get_session'),

  connect: (keyId: string, secret: string, mode: 'paper' | 'live') =>
    invoke<SessionInfo>('connect', { req: { keyId, secret, mode } }),

  disconnect: () => invoke<SessionInfo>('disconnect'),

  getOverview: () => invoke<Overview>('get_overview'),

  getAccount: () => invoke<Account>('get_account'),

  getClock: () => invoke<Clock>('get_clock'),

  listPositions: () => invoke<Position[]>('list_positions'),

  closePosition: (symbol: string) => invoke<unknown>('close_position', { symbol }),

  listOrders: (status: 'open' | 'closed' | 'all' = 'all', limit = 100) =>
    invoke<Order[]>('list_orders', { status, limit }),

  createOrder: (req: CreateOrderRequest) => invoke<Order>('create_order', { req }),

  cancelOrder: (id: string) => invoke<{ canceled: string }>('cancel_order', { id }),

  listAssets: (q = '', limit = 25, refresh = false) =>
    invoke<AssetsPage>('list_assets', { q, limit, refresh }),

  getAsset: (symbol: string) => invoke<unknown>('get_asset', { symbol }),

  getBars: (symbol: string, timeframe: string, days: number) =>
    invoke<BarSeries>('get_bars', { symbol, timeframe, days }),

  getQuote: (symbol: string) => invoke<Quote>('get_quote', { symbol }),

  getFeedStatus: () => invoke<FeedStatus>('feed_status'),

  setStreamSymbol: (symbol: string) => invoke<void>('set_stream_symbol', { symbol }),

  runBacktest: (req: BacktestRequest) =>
    invoke<BacktestResult>('run_backtest', { req }),

  getUiState: () => invoke<UiState>('get_ui_state'),

  saveUiState: (state: UiState) => invoke<void>('save_ui_state', { req: state }),

  getStorageInfo: () => invoke<StorageStats>('get_storage_info'),
}

/**
 * Tauri events carry `{ event, id, payload }`; the app only wants the payload.
 * Kept here so components never import `@tauri-apps/api/event` directly.
 */
export function onEvent<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (message) => handler(message.payload))
}

/**
 * Tauri rejects with the Rust `String`, but a transport hiccup can surface as
 * an Error or a bare object, so normalise everything to a displayable message.
 */
export function apiError(err: unknown): string {
  if (typeof err === 'string') return err
  if (err instanceof Error) return err.message
  if (err && typeof err === 'object' && 'message' in err) {
    const message = (err as { message?: unknown }).message
    if (typeof message === 'string') return message
  }
  return 'Request failed.'
}

// ---------------------------------------------------------------------------
// Formatting helpers shared by the panes
// ---------------------------------------------------------------------------

export function num(value: unknown): number | null {
  if (value === null || value === undefined || value === '') return null
  const n = typeof value === 'number' ? value : Number(value)
  return Number.isFinite(n) ? n : null
}

export function fmtMoney(value: unknown, digits = 2): string {
  const n = num(value)
  if (n === null) return '—'
  return n.toLocaleString('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  })
}

export function fmtNum(value: unknown, digits = 2): string {
  const n = num(value)
  if (n === null) return '—'
  return n.toLocaleString('en-US', {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  })
}

/** Alpaca's snake_case enums: `stop_limit` -> `Stop limit`. */
export function sentenceCase(value: string): string {
  const lower = value.replace(/_/g, ' ').toLowerCase()
  return lower.charAt(0).toUpperCase() + lower.slice(1)
}

/** Share counts: up to four decimals, but never trailing zeroes. */
export function fmtQty(value: unknown): string {
  const n = num(value)
  if (n === null) return '—'
  return n.toLocaleString('en-US', { maximumFractionDigits: 4 })
}

export function signed(value: number, digits = 2): string {
  const sign = value > 0 ? '+' : value < 0 ? '−' : ''
  return sign + fmtNum(Math.abs(value), digits)
}
