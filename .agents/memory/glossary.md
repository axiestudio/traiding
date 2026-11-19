# Glossary

Terms used in code, comments and the UI.

| Term | Meaning here |
| --- | --- |
| **paper** | Alpaca's simulated environment, `paper-api.alpaca.markets`. Keys start `PK`. No real money. |
| **live** | Real money, `api.alpaca.markets`. Keys start `AK`. Selected by the Live toggle. |
| **equity** | Account value: cash + long market value − short market value. |
| **buying power** | What you can still deploy. For a margin account it exceeds cash. |
| **notional** | Order size in **dollars** instead of shares. Fractional shares allowed. |
| **qty** | Order size in **shares**. Mutually exclusive with `notional`. |
| **TIF** | Time in force: `day`, `gtc` (until cancelled), `ioc` (immediate-or-cancel), `fok` (fill-or-kill). |
| **stop / stop-limit** | Stop triggers at `stop_price`; a stop-**limit** then rests with a `limit_price`. |
| **unrealized P/L** | Open profit/loss on held positions: (last − avg entry) × qty. |
| **avg entry** | Average price paid across the open position. |
| **blotter** | The positions/orders table pane. Trading-desk term for the trade log. |
| **OHLCV** | Open, High, Low, Close, Volume — one candle. |
| **IEX** | The market-data feed on Alpaca's free plan. Delayed/partial vs full SIP. |
| **bars** | Candles. `1Min`, `1Hour`, `1Day` here. |
| **quote** | Latest bid/ask pair, not a trade. |
| **trade** | A print: price and size. Moves the last candle and last price. |
| **mid** | `(bid + ask) / 2`. Displayed in the quote strip. |
| **stream** | The IEX websocket (`RealtimeData<IEX>`) and the order-update socket in `src/stream.rs`. |
| **feed live** | At least one websocket is connected. `feed polling` means REST fallback only. |
| **SMA crossover** | Long when fast SMA crosses above slow SMA; flat on the reverse. Signals fill on the next open. |
| **backtest** | A historical run of the strategy over daily bars, stored in SQLite. |
| **drawdown** | Peak-to-trough equity decline in percent; reported as a negative number. |
| **benchmark** | Buy-and-hold return over the same bars, shown against the strategy. |
| **masked key** | `PKTE…3456` — the only form of the credential the webview ever sees. |
| **session** | A validated `Arc<Session>` in `AppState`. Dropped on disconnect. |
| **IPC** | Tauri's in-process bridge behind `invoke(cmd, args)`. No network involved. |
| **command** | A `#[tauri::command]` function callable from the UI. |
| **storage** | The app-local SQLite database at `<app_local_data_dir>/storage/traiding.db`. |
| **WAL** | SQLite write-ahead log; lets reads continue while a snapshot is written. |
| **snapshot** | One point-in-time account/position row appended on each overview poll. |
| **asset** | A tradable instrument (`UsEquity`, `active`). Cached 1 hour. |
| **frame** | A chart timeframe selection: `1D`, `1M`, `6M`, `1Y`, `5Y` (UI name: range). |
