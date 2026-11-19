# traiding

An Alpaca trading terminal as a **single Rust desktop app**. A live IEX chart,
positions, orders, an order ticket and a backtester live in one native window;
there is no server, no port and no browser tab. Credentials are held in the
process and never written to disk. Preferences, market history and backtest
results live in a local SQLite database.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/axiestudio/traiding/main/install.sh | sh
```

That resolves the latest release, downloads the AppImage and drops a launcher in
`~/.local/share/applications`. Linux needs `webkit2gtk-4.1`; the script tells you
the right package name for your distro if it is missing.

Other modes:

```bash
install.sh --from-source             # build with cargo + npm, install locally
install.sh --deb                     # system package instead of AppImage
install.sh --prefix /opt/traiding    # install somewhere else
install.sh --uninstall               # remove it again
```

Pin a release with `TRAIDING_VERSION=v0.2.0`, or point at another fork with
`TRAIDING_REPO=you/traiding`.

## Develop

```bash
npm install
npm run dev        # tauri dev: Vite on :1420, hot reload in the window
npm run build      # release binary + .deb/.AppImage/.rpm in target/release/bundle
```

The window opens and asks for Alpaca keys. Paper trading is the default. The
**Live** toggle switches the base URL to `https://api.alpaca.markets` — real
money, and the dialog says so.

To skip the dialog, put keys in `.env`:

```bash
cp .env.example .env
$EDITOR .env       # add APCA_API_KEY_ID and APCA_API_SECRET_KEY
```

Startup authentication is best-effort: bad or empty credentials log a warning and
the connect dialog appears instead.

## Agent

The **Agent** tab is a BYOK assistant. Put an OpenRouter key in `.env` and
restart:

```bash
OPENROUTER_API_KEY=sk-or-...
```

It defaults to `inclusionai/ling-3.2-flash:free`; set `AGENT_MODEL` to change
it, or `AGENT_REASONING=1` for a hybrid-reasoning model. The key stays in the
Rust process — the webview only receives whether one is configured. The agent
has ten read-only tools (quote, bars, positions, account, orders, snapshot,
news, clock, watchlists, calendar); it cannot place, change or cancel an order.

## Verify

```bash
cargo check --all-targets && cargo test && npm run typecheck
npm run build
```

Rendering is checked separately with the dev-only IPC mock and a real webview
capture. See [`.agents/memory/verification.md`](.agents/memory/verification.md).

## Local data

The app creates one SQLite database on first launch, using `rusqlite` with the
bundled SQLite build and WAL mode:

| Platform | Path |
| --- | --- |
| Linux | `~/.local/share/dev.traiding.terminal/storage/traiding.db` |
| macOS | `~/Library/Application Support/dev.traiding.terminal/storage/traiding.db` |
| Windows | `%LOCALAPPDATA%\dev.traiding.terminal\storage\traiding.db` |

It stores the last symbol/range/tab, fetched candles, account and position
snapshots, order history, the last quote per symbol and every completed backtest.
Credentials are **not** in the database. Uninstalling leaves it in place;
deleting the `storage` folder resets local state and history.

## Tools

`tools/context7/ctx7` queries current library docs from the terminal;
`npm run ui:dev` serves the UI in a browser with a dev-only IPC mock;
`tools/fetch-alpaca-cli.sh` downloads the Alpaca CLI sidecar. See
[`tools/README.md`](tools/README.md).

## What it does

| Pane | Content |
| --- | --- |
| Top bar | Paper/live badge, market clock, live-feed indicator, equity, buying power, disconnect |
| Chart | TradingView Lightweight Charts candles + volume, crosshair OHLCV legend, `1D`/`1M`/`6M`/`1Y`/`5Y` ranges, realtime IEX bid/ask/mid strip, live last candle |
| Blotter | Positions with unrealized P/L and one-click close; orders with status badges and cancel; refreshes on order events |
| Ticket | Buy/sell, market/limit/stop/stop-limit, shares or notional, TIF, buying-power estimate |
| Backtest | SMA crossover on daily bars, return / benchmark / drawdown / win-rate metrics, equity curve, recent trades |
| Agent | BYOK chat over the account: quotes, bars, positions, orders, news, calendar; read-only tools |

`Ctrl`/`Cmd`+`K` focuses the symbol field. The symbol, chart range and blotter
tab are restored on the next launch.

## Architecture

```
┌──────────────────── one process ─────────────────────┐
│  webview (webkit2gtk)                                │
│  React 19 + Astryx  ──┐   Lightweight Charts         │
│                       │                              │
│            invoke("get_overview") / onEvent(...)     │
│                       │  in-process IPC, no socket   │
│  Rust ────────────────┴──► #[tauri::command]         │
│                     │        apca::Client → Alpaca   │
│                     └──────► websocket streams       │
│                              quotes · trades · orders│
└──────────────────────────────────────────────────────┘
```

- **No HTTP server.** The UI calls Rust functions directly through Tauri's IPC,
  so there is no listening port, no CORS layer and nothing to reach from outside.
- **Realtime inside Rust.** `src/stream.rs` owns the IEX quote/trade socket and
  the order-update socket, reconnecting with bounded backoff. Prices reach the
  webview as Tauri events; credentials never do. REST polling stays as fallback.
- **Backtests run locally.** `src/backtest.rs` fetches daily bars through the same
  `apca` client, executes SMA crossover signals on the next open, and stores the
  result JSON in SQLite.
- **The agent stays in Rust.** `src/agent.rs` streams OpenRouter SSE, adds the
  paper/live system prompt and runs every tool in-process, so the key never
  reaches the webview. Tool calls render inline with the reply; failures render
  as a sentence instead of an empty bubble.
- **Credentials never reach the webview.** The UI sends them once; Rust validates
  them against `/v2/account` and keeps them in `AppState` behind an `RwLock`. Only
  a masked key (`PKTE…3456`) is ever returned, and Disconnect drops the session.
  Credentials are never persisted.
- **Local data is SQLite.** Preferences, candles, snapshots, orders and quotes
  go to the app-local data directory through `rusqlite`. A database failure
  logs a warning and leaves live trading unaffected.
- **Every command returns `Result<T, String>`**, so a rejected promise in the UI
  already carries a clean message. [`src/error.rs`](src/error.rs) unwraps
  `HTTP status 422 …: {"code":…,"message":…}` down to Alpaca's own sentence.
- **Decimals are lossless.** Prices arrive as JSON numbers and are converted
  through their decimal string into `num_decimal::Num`, so upstream never sees
  float artefacts like `0.30000000000000004`.
- **Assets are cached for an hour** in the Rust process, since `/v2/assets`
  returns ~11k rows.

## Layout

```
src/main.rs        thin binary → traiding_lib::run()
src/lib.rs         window setup, env-credential bootstrap, storage + commands
src/commands.rs    IPC handlers for session, account, orders, bars, quotes
src/stream.rs      IEX quote/trade + order-update websockets → Tauri events
src/backtest.rs    SMA crossover engine, metrics, equity curve, run command
src/state.rs       session, streams, asset cache, storage handle
src/storage.rs     SQLite schema, migrations, snapshots, backtests
src/error.rs       error mapping
src/agent.rs       OpenRouter SSE, read-only tool loop, system prompt
src/cli.rs         bundled Alpaca CLI child process for agent tools
ui/src/            React app (api.ts is the IPC boundary)
ui/src/agent/      assistant-ui chat surface (markdown, tool calls, errors)
ui/src/components/ TopBar, ConnectDialog, ChartPanel, PriceChart, Positions, Orders, Ticket, Blotter, BacktestPanel, EquityChart
ui/src/dev/        browser-only IPC mock, never bundled
tools/             Context7 CLI + Alpaca CLI fetch
tauri.conf.json    window, CSP, bundle targets
install.sh         curl installer
.agents/           agent skills + project memory wiki
```

## Limitations

- Linux only for now: the bundle targets are `deb`, `appimage` and `rpm`, and the
  installer probes for `webkit2gtk-4.1`. The Rust and React code itself is
  portable.
- US equities only; the symbol parser is scoped to that.
- Market data uses the IEX feed, which is what the free plan provides. The
  quote/trade socket is IEX-only; bars are regular-session only.
- REST polling remains at 15s/20s as a fallback when the socket is offline.
- Backtesting supports one long-only strategy (SMA crossover), daily bars and
  one symbol at a time. It is not an optimiser or a portfolio simulator.
- The agent needs an OpenRouter key and is read-only; free models rate-limit.
- The Alpaca CLI is fetched into `binaries/` and shipped as an app resource at
  `usr/lib/traiding/bin/alpaca`, never on `PATH`. Only the agent's Rust-side
  tools spawn it, and only for snapshot, news, clock, watchlists and calendar.
- Local snapshots and backtests are append-only; there is no retention or vacuum
  policy yet.
