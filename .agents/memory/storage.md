# Local storage

## Why SQLite

Use `rusqlite` with the **bundled** SQLite build. One embedded file, real
transactions, WAL readers, indexes on `(symbol, timeframe, ts)`, and no server or
extra process to ship. It handles years of daily candles and append-only account
snapshots without a redesign.

Rejected:

- JSON/CSV files — no atomic multi-table writes, no indexed range queries.
- A separate Postgres — contradicts the single-binary constraint.
- LocalStorage in the webview — credentials and history would leave Rust, and it
  has no query layer.

## Location

`app.path().app_local_data_dir()`, then `storage/traiding.db`.

| Platform | Path |
| --- | --- |
| Linux | `~/.local/share/dev.traiding.terminal/storage/traiding.db` |
| macOS | `~/Library/Application Support/dev.traiding.terminal/storage/traiding.db` |
| Windows | `%LOCALAPPDATA%\dev.traiding.terminal\storage\traiding.db` |

The directory is independent of the install prefix, so `.deb`, AppImage and
`install.sh --from-source` all share one database.

## Schema v2

| Table | Key | Holds |
| --- | --- | --- |
| `settings` | `key` | `ui_state` JSON: symbol, range, blotter tab |
| `bars` | `(symbol, timeframe, ts)` | OHLCV, upserted per fetch |
| `account_snapshots` | `id` | one row per overview poll: equity, cash, buying power |
| `position_snapshots` | `id` | one row per open position per poll |
| `orders` | `id` | latest known order, upserted |
| `quotes` | `symbol` | latest bid/ask/mid per symbol |
| `backtests` | `id` | one row per run: symbol, strategy, params JSON, result JSON |

Index `backtests_symbol_time` on `(symbol, created_at)` for recent-run queries.

`PRAGMA user_version` is the migration marker. To change the schema, bump
`SCHEMA_VERSION` and add forward-only DDL; the `CREATE ... IF NOT EXISTS` batch is
idempotent.

## Rules

- **No credentials.** The database never stores a key, secret or header. This is
  enforced by keeping `Session` in memory, by `Storage` having no session API,
  and by routing the app-config `.env` through `src/config.rs`.
- **Open once.** `lib.rs` opens it during setup before `app.manage(state)`;
  `AppState` holds `Arc<Storage>`, and every command clones that handle.
- **WAL + 5 s busy timeout.** A checkpoint does not block a reader; a writer
  waits instead of failing immediately.
- **Best-effort writes.** Call sites log `tracing::warn!` and return the live
  data. Never make a dashboard poll fail because persistence failed.
- **All SQL lives in `storage.rs`.** Commands pass `Value`s, not SQL.
- **Snapshots are append-only.** No retention policy yet; `account_snapshots`
  and `position_snapshots` grow by one set per 15 s poll while connected.
  `backtests` also grows one row per run; the result JSON is stored whole.

## Inspect it

```bash
sqlite3 ~/.local/share/dev.traiding.terminal/storage/traiding.db \
  '.tables' 'PRAGMA user_version' 'SELECT COUNT(*) FROM bars;'
```

Deleting the `storage` folder is the supported reset. It does not touch the
app-config `.env`, which holds the Alpaca and OpenRouter keys at mode 0600.
