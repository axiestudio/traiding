# traiding

An Alpaca trading terminal as a **single Rust desktop app**. A live IEX chart,
positions, orders, an order ticket and a backtester live in one native window;
there is no server, no port and no browser tab. Credentials never reach the
webview; a successful connect stores the Alpaca key pair in the app-config
`.env` (0600). Preferences, market history and backtest results live in a local
SQLite database; the database never holds a secret.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/axiestudio/traiding/main/install.sh | sh
```

Downloads the latest AppImage into `~/.local` and adds a launcher. Linux needs
`webkit2gtk-4.1`; the script names your distro's package if it is missing.

```bash
install.sh --from-source    # build locally
install.sh --deb            # system package
install.sh --uninstall      # remove
```

`TRAIDING_VERSION=v0.2.0` pins a release; `TRAIDING_REPO=you/traiding` uses a fork.

## Develop

```bash
npm install
npm run dev        # tauri dev: Vite on :1420, hot reload in the window
npm run build      # release binary + .deb/.AppImage/.rpm in target/release/bundle
```

The window opens and asks for Alpaca keys. Paper trading is the default. The
**Live** toggle switches the base URL to `https://api.alpaca.markets` — real
money, and the dialog says so.

The first successful connect writes `APCA_API_KEY_ID`, `APCA_API_SECRET_KEY`
and `APCA_API_BASE_URL` to `~/.config/dev.traiding.terminal/.env`, so the next
launch connects without the dialog. Disconnect only clears the session; delete
that file to forget the keys.

To script a connection instead, put keys in the project `.env`:

```bash
cp .env.example .env
$EDITOR .env       # add APCA_API_KEY_ID and APCA_API_SECRET_KEY
```

Startup authentication is best-effort: bad or empty credentials log a warning and
the connect dialog appears instead.

## Agent

BYOK chat in the **Agent** tab; enter the OpenRouter key there. Rust validates it
and saves it to `~/.config/dev.traiding.terminal/.env`.

Defaults to `inclusionai/ling-3.2-flash:free`; `AGENT_MODEL` changes it,
`AGENT_REASONING=1` uses a reasoning model. Read-only.

## Verify

```bash
cargo check --all-targets && cargo test && npm run typecheck
```

Rendering: [`.agents/memory/verification.md`](.agents/memory/verification.md).

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
Credentials are **not** in the database; they live only in the app-config
`.env` (0600). Uninstalling leaves both in place; deleting the `storage` folder
resets local state and history.

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

`Ctrl`/`Cmd`+`K` opens the symbol selector. The symbol, chart range and blotter
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
- **The agent stays in Rust.** `src/agent.rs` streams OpenRouter SSE and runs
  every tool in-process; the key lives in Rust.
- **Credentials never reach the webview.** The UI sends them once; Rust validates
  them against `/v2/account` and keeps them in `AppState` behind an `RwLock`. Only
  a masked key (`PKTE…3456`) is ever returned. After a successful connect the key
  pair is written to the app-config `.env` (0600) for the next launch; Disconnect
  drops the session but leaves that file. SQLite never stores a secret.
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
src/config.rs      app-config .env merge/write at 0600
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
- Market data uses the IEX feed, the only realtime equities feed on Alpaca's
  Basic plan. SIP is 15 minutes delayed on Basic and realtime on Algo Trader
  Plus. The quote/trade socket is IEX-only; bars are regular-session only.
- REST polling remains at 15s/20s as a fallback when the socket is offline.
- Backtesting supports one long-only strategy (SMA crossover), daily bars and
  one symbol at a time. It is not an optimiser or a portfolio simulator.
- The agent needs an OpenRouter key and is read-only; free models rate-limit.
- The Alpaca CLI is fetched into `binaries/` and shipped as an app resource at
  `usr/lib/traiding/bin/alpaca`, never on `PATH`. Only the agent's Rust-side
  tools spawn it, and only for snapshot, news, clock, watchlists and calendar.
- Local snapshots and backtests are append-only; there is no retention or vacuum
  policy yet.
