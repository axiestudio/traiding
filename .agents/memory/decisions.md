# Decisions

## Tauri 2, IPC only — not a local server

The previous version was an axum server on `127.0.0.1:8080` serving a
hand-written 1769-line HTML page. Rejected keeping it because:

- a listening port is an attack surface on a machine holding trading keys;
- `curl` scripts and browser tabs are the wrong shape for a desktop tool;
- bundled assets + IPC remove CORS, port conflicts and orphaned server processes.

Cost: no remote/curl API. Accepted — this is a local terminal, not a service.

## Astryx (`@astryxdesign/core`) — not Tailwind, not shadcn

Chosen because it ships **prebuilt CSS + JS**, so no build plugin and no
StyleX compiler config; and it carries real data components (`Table`,
`SegmentedControl`, `Selector`, `Toast`) that a trading dashboard needs.

Known risk: it is **Beta** (v0.7.16). Mitigation: every prop is read from the
shipped `.d.ts` before use, and the failure modes are recorded in
[gotchas.md](gotchas.md). Tailwind was explicitly declined — no fallback layer.

## Lightweight Charts v5 — not a React chart wrapper

Apache-2.0, canvas, zero React coupling, ~45 KB. A React wrapper would add a
dependency for no gain since the chart is one imperative mount/unmount.

v5 API differs from v4: `chart.addSeries(CandlestickSeries, opts, paneIndex)`,
not `chart.addCandlestickSeries(...)`.

## `apca` crate — not hand-rolled HTTP

Handles auth headers, paper/live hosts, and typed endpoints (`orders`, `bars`,
`assets`, `last_quotes`). Its error type surfaces as
`HTTP status 422 …: {"code":…,"message":…}`, which `error.rs` unwraps to the
plain Alpaca sentence.

## SQLite via `rusqlite` (bundled) — not flat files

The app needs years of candles, order history and account snapshots on one
machine. SQLite gives one embedded file, transactions, indexed range queries and
WAL readers without a server. `rusqlite` with the `bundled` feature compiles
SQLite 3.54 into the binary (no system `libsqlite3` dependency); see
[storage.md](storage.md) for the schema.

Rejected: JSON/CSV files (no atomic multi-table writes or indexed queries),
Postgres (a server violates the single-binary rule), webview storage (history
would leave Rust). Credentials are never stored in the database; the Alpaca
pair lives only in the app-config `.env` at mode 0600.

## `num_decimal::Num` for prices

Prices arrive as JSON numbers (`f64`). Sending `f64` upstream risks
`0.30000000000000004`. Conversion is `f64 → decimal string → Num`, never
arithmetic, because `Num`'s `Display` is the exact decimal it holds.

## WebSocket in Rust, events into the webview

`apca::data::v2::stream::RealtimeData<IEX>` and `OrderUpdates` run in
`src/stream.rs`; the webview only receives Tauri events. That keeps credentials
out of the webview and avoids a second process or HTTP bridge. The market loop
holds one socket and repoints its subscription through a `watch` channel, so a
symbol change does not reconnect. Backoff is 2–30 s; REST polling at 15 s/20 s
stays as a fallback.

## SMA crossover backtest, no Python/sidecar

The backtest runs in Rust against daily `apca` bars and stores its result JSON in
SQLite. A Python/quant stack would add a runtime and a second data path for one
long-only strategy. Signals execute on the next bar's open; the benchmark is
buy-and-hold. The strategy set is intentionally one until the loop is proven.


## Feature parity, not a redesign

The port kept every behaviour of the old UI (5 ranges, same columns, same
validation) so the risk was in the stack, not in changed semantics. New
capabilities (watchlist, palette, autocomplete) are deliberately deferred.

## Rejected

| Option | Why not |
| --- | --- |
| Electron | ~100 MB runtime, separate Node process — the opposite of "one binary" |
| Tauri + HTTP loopback | keeps the port and CORS problems, gains nothing over IPC |
| Tailwind on top of Astryx | two styling systems; user preferred Astryx alone |
| Server-side rendering the UI | no benefit in a single-user local app |
