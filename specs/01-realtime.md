# Realtime market and order data

Status: accepted for implementation.

## Goal

While a session is connected, the terminal shows the selected symbol's IEX quote
and last trade as they happen, and refreshes positions/orders when Alpaca reports
an order event. REST polling remains a fallback, not the primary feed.

## Process and security

- One process. Tauri IPC only; no listening HTTP port.
- Only the Rust side opens the Alpaca websocket. The API key and secret stay in
  `Session`; the webview receives events with prices only.
- Streams are owned by `AppState`. Connect starts them, disconnect aborts them,
  and a symbol change repoints the market subscription without reconnecting.
- `apca`'s `Client::subscribe` is used directly. The bundled Alpaca CLI stays a
  sidecar for diagnostics and is not a subprocess dependency of the feed.

## Market stream

- Source: `RealtimeData<IEX>`. Subscribe to `quotes` and `trades` for exactly one
  symbol.
- Events emitted to the UI:
  - `market:quote` — `{ symbol, time, bid, ask, bidSize, askSize, mid }`
  - `market:trade` — `{ symbol, price, size, time }`
  - `stream:status` — `{ channel: "market", state: "connecting" | "live" | "offline" }`
- Time is RFC 3339. Prices are numbers; sizes are whole-share numbers.
- Unknown or late events for a previous symbol are ignored by the UI.
- Reconnect: exponential backoff from 2 s to 30 s. On reconnect, resubscribe to
  the current symbol (the latest value in a `watch` channel).
- A stream failure never turns the dashboard into an error screen; polling keeps
  working and the feed indicator says offline.

## Order stream

- Source: `OrderUpdates`; authentication and `trade_updates` subscription are
  handled inside `apca`.
- Event: `trade:update` with `{ event, order }`. `event` is Alpaca's order
  status name; `order` is the SDK order JSON.
- UI response: refresh the overview (debounced 400 ms) and toast only when an
  order reaches `filled`, `canceled`, `rejected` or `expired`.
- Status event uses `channel: "orders"`.

## Lifecycle

| Moment | Market | Orders |
| --- | --- | --- |
| Environment pre-auth at startup | starts when UI sends the symbol | starts at setup |
| `connect` | starts when UI sends the symbol | starts immediately |
| symbol change | unsubscribe old, subscribe new | unchanged |
| `disconnect` | aborted | aborted |

`set_stream_symbol` is idempotent: the same symbol does nothing, a new symbol
repoints the subscription, no session is a no-op rather than an error.

## UI contract

- App listens once per connection, throttles quote-driven renders to 4 Hz, and
  updates the header price, quote strip and last candle's close/high/low.
- Quote strip shows `IEX live` with a visible dot when `market` is live, `IEX`
  otherwise. The status is text plus colour, never colour alone.
- Polling stays at 15 s overview / 20 s quote. It is the fallback and the
  reconnect safety net.

## Acceptance

1. With a paper session, a quote event changes the bid/ask within one second
   during market hours.
2. A trade event moves the last price and the last candle without a chart reload.
3. Placing an order produces a `trade:update`; a fill refreshes the blotter.
4. Disconnecting ends both sockets; no events arrive afterwards.
5. `cargo check --all-targets` and `npm run typecheck` are clean.

## Non-goals

- SIP feed, options/crypto streams, and full order-book depth.
- Persisting every tick. Only the latest quote is stored, as today.
