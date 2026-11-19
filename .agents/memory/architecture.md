# Architecture

## One process, one binary

```
┌────────────────────── traiding (ELF, 7.7 MB) ───────────────────────┐
│  webkit2gtk webview                                                 │
│    React 19 · Astryx · Lightweight Charts                           │
│         │                                                           │
│         │  invoke("get_overview")   ◄── no socket, no port          │
│         ▼                                                           │
│  Rust                                                               │
│    #[tauri::command] handlers  →  apca::Client  →  Alpaca HTTPS     │
│    stream.rs → IEX quotes/trades + order updates → app.emit(...)     │
│    AppState { RwLock<Option<Arc<Session>>>, stream handles, cache }  │
└─────────────────────────────────────────────────────────────────────┘
```

Assets are compiled in. There is nothing to serve and nothing to bind.

## The tree

```
traiding/
├── AGENTS.md                  repo rules for agents (read-first table)
├── Cargo.toml                 Rust crate "traiding", lib "traiding_lib"
├── Cargo.lock                 pinned dependency graph
├── build.rs                   tauri_build::build() → reads tauri.conf.json
├── tauri.conf.json            window 1440×900 (min 1080×720), CSP, bundles
├── package.json               owns @tauri-apps/cli; delegates to ui/ scripts
├── install.sh                 curl installer (AppImage / deb / rpm / --from-source)
├── README.md                  user-facing intro
├── specs/                     realtime, backtesting and desktop UI specs
│
├── src/                       ── Rust backend ──
│   ├── main.rs          (10)   just calls traiding_lib::run()
│   ├── lib.rs                  tracing, storage open, env pre-auth, window, registry
│   ├── commands.rs             IPC handlers — session, account, orders, bars, quotes
│   ├── stream.rs               IEX quote/trade + order-update websocket loops
│   ├── backtest.rs             SMA crossover engine + run_backtest command
│   ├── state.rs                AppState: Session, stream handles, asset cache, Storage
│   ├── storage.rs              bundled SQLite: schema v2, migrations, snapshots, backtests
│   ├── agent.rs                BYOK chat agent: OpenRouter SSE, tool loop, CLI tools
│   ├── cli.rs                  runs the bundled `alpaca` child with session credentials
│   └── error.rs                AppError → String, unwraps `HTTP status 422 {"message":…}`
│
├── ui/                        ── React frontend ──
│   ├── index.html              single #root div
│   ├── vite.config.ts          port 1420 strict, minify: 'oxc' (Vite 8)
│   ├── tsconfig.json           tsc --noEmit config
│   └── src/
│       ├── main.tsx            createRoot + <Theme>; dev-only IPC mock bootstrap
│       ├── App.tsx             ⭐ session, event listeners, polling, symbol/range, rail tab
│       ├── api.ts              ⭐ the ONLY file that calls invoke/listen; wire types
│       ├── dev/mockTauri.ts    browser-only fake IPC, installed only under Vite
│       ├── styles.css          terminal grid + rail + backtest + agent chrome
│       ├── agent/              ── assistant-ui chat surface (barrel: index.ts) ──
│       │   ├── index.ts           public API; nothing outside imports past it
│       │   ├── AgentPanel.tsx     runtime + configured gate + empty state
│       │   ├── AgentMessage.tsx   role-aware message, parts registry
│       │   ├── AgentComposer.tsx  textarea + send
│       │   ├── adapter.ts         ChatModelAdapter bridging IPC channel → async iter
│       │   ├── ipc.ts             invoke/Channel wrappers, wire types
│       │   ├── format.ts          JSON one-liner + tool label helpers
│       │   └── parts/             TextPart, ToolCallPart + barrel
│       └── components/
│           ├── TopBar.tsx         mode badge, market clock, feed status, equity/buying power
│           ├── ChartPanel.tsx     symbol input, last price, 5 ranges, live quote strip
│           ├── PriceChart.tsx     imperative Lightweight Charts mount (canvas)
│           ├── Blotter.tsx        tabs: positions | orders
│           ├── PositionsTable.tsx holdings + "close" liquidate action
│           ├── OrdersTable.tsx    order history + cancel, status→badge colours
│           ├── OrderTicket.tsx    ⭐ the only place that can spend money
│           ├── BacktestPanel.tsx  SMA form, metrics, trade list
│           ├── EquityChart.tsx    strategy vs buy-and-hold line chart
│           └── ConnectDialog.tsx  non-dismissable gate when not connected
│
├── tools/                     dev helpers, never shipped
│   ├── README.md              explains each tool
│   ├── context7/ctx7          doc-lookup helper
│   └── fetch-alpaca-cli.sh    fetched Alpaca CLI resource + checksum (not PATH)
│
├── temp/                      upstream sources, cloned not vendored
│   ├── README.md              what was cloned, when, at which commit
│   ├── alpaca-mcp-server/     github.com/alpacahq/alpaca-mcp-server
│   └── cli/                   github.com/alpacahq/cli
│
├── icons/                     app icon set (4 paths must exist or build panics)
├── gen/schemas/               Tauri-generated ACL/capability schemas (gitignored)
└── .agents/                   skills + this memory wiki
    ├── memory/                the wiki you are reading
    └── skills/
        ├── astryx/            Astryx design system rules
        ├── copywriting/       user-facing text rules
        ├── rust/              backend conventions
        ├── ui/                frontend conventions
        └── alpaca-trading-*   4 official Alpaca skills (Apache-2.0)
```

`api.ts` is the only file importing `@tauri-apps/api/core`. If another file
needs data, it goes through `api.ts` or it does not get data.

## Layer responsibilities

| Layer | Owns | Never does |
| --- | --- | --- |
| `ui/src/App.tsx` | session, event listeners, polling cadence, symbol/range, action handlers | talk to `invoke` directly |
| `ui/src/api.ts` | every `invoke`/`listen`, wire types, formatting helpers | hold state |
| `ui/src/components/*` | rendering + local form state | fetch, or know about Alpaca |
| `src/commands.rs` | validation, `apca` calls, response shaping | render, or know about React |
| `src/stream.rs` | websocket lifecycle, event payloads, reconnects | expose credentials in payloads |
| `src/backtest.rs` | SMA signals, metrics, equity curve | place orders |
| `src/state.rs` | session lifecycle, stream handles, asset cache, key masking, storage handle | expose raw secrets |
| `src/storage.rs` | SQLite schema, migrations, snapshot writes | store credentials |
| `src/config.rs` | app-config `.env` merge/write at 0600 | touch SQLite or the webview |
| `src/error.rs` | `AppError` → `String`, upstream message unwrapping | log secrets |

## Session lifecycle

1. Startup: `lib.rs` reads `.env`; if keys exist it tries `Session::connect`.
   Failure is a **warning**, not fatal — the dialog appears instead.
2. UI calls `get_session`. If not connected, `ConnectDialog` opens (non-dismissable).
3. `connect` validates against `/v2/account` **before** storing anything.
4. Success stores `Arc<Session>` in `AppState`; UI receives `{connected, paper,
   key_id (masked), base_url}`.
5. A successful connect writes the Alpaca pair to the app-config `.env` (0600);
   the next launch reads it in `load_config_env` and pre-authenticates.
   `disconnect` clears the session but leaves that file. UI preferences and
   market data are persisted in SQLite; no secret is.

## Realtime streams

`connect` (and environment pre-auth) starts the order-update socket. The chart
symbol effect calls `set_stream_symbol`, which starts or repoints the IEX quote/
trade socket. Both loops live in `src/stream.rs`, emit `stream:status`, and
reconnect with 2–30 s backoff. Disconnect aborts both tasks.

IEX is the only realtime equities feed on Alpaca's Basic plan. SIP is 15 minutes
delayed on Basic and realtime on Algo Trader Plus, so every bars and latest-quote
request pins `feed=iex`; the websocket is `RealtimeData<IEX>`.

| Event | Payload |
| --- | --- |
| `market:quote` | symbol, time, bid/ask, sizes, mid |
| `market:trade` | symbol, time, price, size |
| `trade:update` | Alpaca order event + order JSON |
| `stream:status` | `{channel, state}` |

The UI throttles quote/trade renders to 4 Hz. A live trade updates the last
candle in place; `PriceChart` only calls `fitContent` when the bar window changes.

## Polling

| What | Interval | Why |
| --- | --- | --- |
| `get_overview` | 15 s | account + clock + positions + orders in one round trip; fallback and order-event safety net |
| `get_quote` | 20 s | IEX latest quote; separate so a slow quote cannot stall the account |

Chart data refreshes on user action (symbol/range change), not on a timer.
Staggering the two loops keeps well inside Alpaca's rate limits. Streams are the
primary feed; polling continues so a dropped socket never blanks the dashboard.

## Asset cache

`/v2/assets` returns ~11k rows. `AppState` caches them for **1 hour**, guarded by
an `RwLock`. `list_assets(refresh: true)` forces a refetch. The chart's symbol
picker searches this list through `list_assets`; the cache keeps that cheap.

## Local storage

SQLite at `<app_local_data_dir>/storage/traiding.db`; see
[storage.md](storage.md) for the schema and rules. The database is opened once
in `lib.rs` during Tauri setup and attached to `AppState`. It survives
`disconnect`. Schema v2 adds the `backtests` table.

Writes are best-effort. `get_bars`, `get_quote`, `create_order`, `run_backtest`
and each `get_overview` poll persist their result, but a failed write only logs a
warning so the terminal keeps working with a locked or full disk.

## Commands

Every handler returns `Ipc<T> = Result<T, String>`; the `String` is already a
clean sentence from `error.rs`.

| Group | Commands |
| --- | --- |
| Session | `get_session`, `connect`, `disconnect` |
| Account | `get_account`, `get_clock` |
| Positions | `list_positions`, `close_position` |
| Orders | `list_orders`, `create_order`, `cancel_order` |
| Assets | `list_assets`, `get_asset` |
| Market data | `get_bars`, `get_quote`, `set_stream_symbol` |
| Backtest | `run_backtest` |
| Aggregate | `get_overview` |
| Local storage | `get_ui_state`, `save_ui_state`, `get_storage_info` |
| Agent | `agent_status`, `agent_chat` |

`agent_chat` takes a `Channel<StreamEvent>` and pushes `delta`, `toolStart`,
`toolDone` and `done` frames as it works. Tool execution happens in Rust; the
webview never sees a tool name it can invoke on its own, and a Rust-owned system
prompt names the paper/live account plus the read-only boundary. The adapter
keeps text and tool parts in emission order and renders the message error when
OpenRouter rejects a turn. `StreamEvent` uses `rename_all_fields` as well as
`rename_all`, so `is_error` reaches the UI as `isError` (gotcha 24).

## Adding a feature

| I want to… | Touch |
| --- | --- |
| add a backend call | handler in `commands.rs` → register in `lib.rs` → wrapper in `api.ts` → consume in `App.tsx` |
| change a wire struct | `commands.rs` (keep `#[serde(rename_all = "camelCase")]`) + `api.ts` interface + the contract test |
| add a pane | new file in `components/`, add `.pane--x` + `grid-template-areas` in `styles.css` |
| change layout proportions | `styles.css` `.terminal`: `340px` ticket column, `248px` blotter row |
| add persistence | `storage.rs`, then bump `SCHEMA_VERSION`; rules in [storage.md](storage.md) |
| change startup auth | `lib.rs` `preauthenticate` |
| add an agent tool | `tool_specs()` + a `match` arm in `dispatch` (`src/agent.rs`) |
| add a CLI-backed agent tool | a `cli_*` arm in `dispatch_cli` (`src/agent.rs`) |

## Bundle

| Target | Command |
| --- | --- |
| `deb` | default |
| `appimage` | default, and what `install.sh` fetches |
| `rpm` | default |

Each package installs the binary to `usr/bin/traiding` and the CLI to
`usr/lib/traiding/bin/alpaca` (see below). Nothing else is written outside the
package's own directories.

Release profile: LTO, `opt-level = "s"`, `strip`, `panic = "abort"`,
`codegen-units = 1`. A cold build takes ~1–10 min; an incremental one ~2 min.

## Bundled Alpaca CLI

The app ships the official `alpaca` CLI inside every package. It is a 9.6 MB
statically linked Go binary, so it needs no runtime and no system install.

| Piece | Where |
| --- | --- |
| Fetch script | `tools/fetch-alpaca-cli.sh` |
| Staged binary | `binaries/alpaca` (gitignored) |
| Tauri config | `bundle.resources: { "binaries/alpaca": "bin/alpaca" }` |
| `deb` / `rpm` | `usr/lib/traiding/bin/alpaca` — **not** on `PATH` |
| AppImage | `usr/lib/traiding/bin/alpaca` inside the AppDir |

It is a **resource, not an `externalBin`**. Tauri installs `externalBin` to
`/usr/bin` by default, which would put `alpaca` on every user's `PATH` and let
it shadow an `alpaca` they installed themselves. A resource stays inside the
app's own directory, so the package touches nothing outside it.

`npm run build` and `npm run dev` both run `cli:fetch` first, so a clean
checkout cannot build without the binary. The script pins `0.0.15`, verifies the
upstream SHA-256 before extracting, and is idempotent — an existing binary is
left alone.

Verified 2026-11-19: the `deb` contains `usr/lib/traiding/bin/alpaca` and no
`usr/bin/alpaca`; the extracted binary reports `0.0.15`.

### Who may run it

`src/cli.rs` is the only caller. It resolves `$RESOURCES/bin/alpaca`, falling
back to `binaries/alpaca` in debug builds only, and runs the child with a
scoped environment: `ALPACA_API_KEY`/`ALPACA_SECRET_KEY` come from the live
`Session` (already in the `apca` client), `ALPACA_LIVE_TRADE` matches the
session's mode, and the rest of the parent environment is cleared so a stray
`ALPACA_PROFILE` cannot redirect a call.

The CLI is **not** spawned from the webview. There is no `tauri-plugin-shell`
and there must not be: the only path to a process is the agent's tool dispatch,
which is Rust-side and holds the credentials. The stream and backtest paths
still use `apca` in-process; the CLI exists for the agent's read-only tools
(snapshot, news, clock, watchlists, calendar), which cover endpoints `apca`
does not wrap.
