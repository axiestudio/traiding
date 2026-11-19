---
name: rust
description: "Use when changing the Rust side of traiding: Tauri commands, the apca client, session/asset state, error mapping, or decimal handling. Covers the IPC contract, how credentials are kept out of the webview, and the field-naming rule that has already broken the connect flow once."
disable-model-invocation: false
metadata:
  version: 1.0.0
---

# Rust side of traiding

One binary. `src/main.rs` is a two-line shim; `src/lib.rs` builds the window and
registers commands; `src/commands.rs` holds all business logic.

```
main.rs     fn main() { traiding_lib::run() }
lib.rs      dotenv, tracing, setup, storage, invoke_handler, .run()
commands.rs session/account/orders/market-data handlers + validation
stream.rs   IEX quote/trade + order-update websocket loops → app.emit
backtest.rs SMA crossover engine, metrics, run_backtest
state.rs    Session::connect, stream handles, AssetRow, 1h cache, Arc<Storage>
storage.rs  bundled SQLite schema v2, migrations, snapshots, backtests
error.rs    AppError → String, upstream message unwrapping
```

There is **no HTTP server**. Do not add axum, actix, or a TCP listener — the
webview talks to Rust through Tauri IPC.

## Command signature

```rust
type Ipc<T> = Result<T, String>;

#[tauri::command]
pub async fn list_orders(
    state: State<'_, Arc<AppState>>,
    status: Option<String>,
    limit: Option<usize>,
) -> Ipc<Value> { … }
```

Every command returns `Ipc<T>`. Tauri converts the `Err` string into a rejected
promise, so the UI receives an already-clean sentence — never a Rust `Debug`
dump. `AppError` implements `From<AppError> for String`.

## The IPC naming rule — read this before editing a struct

**Tauri camelCases top-level command argument names** (`invoke_message` is
`invokeMessage`). Nested request structs are separate: they need
`#[serde(rename_all = "camelCase")]`, and the TypeScript must match the
resulting field names exactly. The two sides are edited independently, so a
mismatch is silent until runtime.

Deserialized request structs use `rename_all = "camelCase"` and the TypeScript
must match **exactly**:

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectReq { pub key_id: String, … }   // wire name: keyId
```

```ts
invoke('connect', { req: { keyId, secret, mode } })   // must be keyId
```

This exact mismatch shipped a bug: the UI sent `key_id`, Rust wanted `keyId`,
and the dialog failed with ``missing field `keyId` ``. It is now pinned by tests
in `commands::tests` — when you change a wire struct, update those tests in the
same commit.

Note the argument **wrapper**: a struct parameter named `req` is passed as
`{ req: { … } }`, not spread at the top level.

## Credentials

- Validated against `/v2/account` **before** being stored.
- Held in `AppState` behind an `RwLock`, in memory only. **Never write them to
  disk**, never log them, never `Debug`-print a `Session`.
- Only `masked_key()` (`PKTE…3456`) is ever returned to the webview.
- `disconnect` drops the session; the UI must not need the raw key for anything.
- The SQLite database stores preferences and market data, **never** a key,
  secret, header or session. `Storage` has no session API for this reason.

If a change would send a secret back over IPC or into SQLite, that change is
wrong.

## Streams

- `stream.rs` owns both sockets. `Client::subscribe::<RealtimeData<IEX>>()` and
  `Client::subscribe::<OrderUpdates>()`; the webview gets events, never keys.
- The market loop subscribes to quotes **and** trades for one symbol. A watch
  channel carries symbol changes; use `stream::drive(Box::pin(..), &mut stream)`
  for every subscribe/unsubscribe control message.
- Event payloads are camelCase (`bidSize`, `askSize`, `mid`). Emit
  `stream:status` with `channel` and `state` on every transition.
- `AppState` stores the task handles and aborts them on disconnect. Do not
  spawn a second market socket per symbol change.
- Reconnect backoff is 2 s → 30 s. A failed socket must not fail a command.

## Backtest

`backtest.rs` is pure simulation plus one command. Signals are computed at the
close and executed at the **next bar's open**. Fees are basis points of notional.
`run_backtest` fetches daily bars through `commands::daily_bars`, returns the
result even if SQLite is unavailable, and stores the whole result JSON.
Test the engine with crafted bar arrays; do not hit the network in tests.

## Decimals

Prices cross IPC as `f64` (JSON numbers) but must reach Alpaca losslessly:

```rust
num_from_f64(value)   // f64 → decimal string → num_decimal::Num
decimal_to_f64(&num)  // display only, never sent upstream
```

Never construct a `Num` from an `f64` via arithmetic — `0.1 + 0.2` artefacts end
up in live orders.

## Local storage

`storage.rs` runs **bundled** `rusqlite` in WAL mode. The database lives at
`app_local_data_dir()/storage/traiding.db` and is opened once in `lib.rs` setup:

```rust
let state = Arc::new(AppState::new());
open_storage(app.handle(), &state);
preauthenticate(&state);
app.manage(state);
```

- `Storage` is `Mutex<Connection>` + the path; `AppState` holds `Arc<Storage>`.
- Migrations use `PRAGMA user_version` and an idempotent `CREATE ... IF NOT
  EXISTS` batch. Bump `SCHEMA_VERSION` for any change.
- `get_bars`, `get_quote`, `create_order`, `run_backtest` and `get_overview`
  persist their result. Every write is best-effort: `tracing::warn!` and return
  the live data, never fail the command.
- `settings` stores `ui_state` as JSON; `UiState::sanitized()` validates anything
  read from disk before it reaches the UI.
- See [`memory/storage.md`](../../memory/storage.md) for the schema.

## Errors

`error.rs` unwraps `apca`'s `HTTP status 422 …: {"code":…,"message":…}` down to
Alpaca's own sentence, and maps 401/403 to `AppError::Unauthorized` so the
message reads as an auth problem. Add upstream statuses there, not at call sites.

## Style

- `cargo check --all-targets` must stay at **zero warnings**; the tree is clean.
- Prefer `?` with `From` conversions over manual `match` on `Result`.
- Keep `commands.rs` handlers thin: validate, call `apca`, shape the response.

## Verify

```bash
cargo check --all-targets
cargo test                     # backtest engine + IPC contract + storage tests
cargo clippy --all-targets
```
