//! Local-only persistence.
//!
//! SQLite is the right fit here: one embedded file, real transactions, indexed
//! time-series queries, and no server. The database lives in Tauri's
//! app-local data directory, never in the repo and never next to credentials.
//!
//! Writes are intentionally best-effort at call sites: a locked or full disk
//! must not stop the terminal from showing live market data.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::{Bar, Quote};
use crate::error::AppError;

const SCHEMA_VERSION: i64 = 2;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS bars (
    symbol    TEXT NOT NULL,
    timeframe TEXT NOT NULL,
    ts        INTEGER NOT NULL,
    open      REAL NOT NULL,
    high      REAL NOT NULL,
    low       REAL NOT NULL,
    close     REAL NOT NULL,
    volume    REAL NOT NULL,
    PRIMARY KEY (symbol, timeframe, ts)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS account_snapshots (
    id              INTEGER PRIMARY KEY,
    captured_at     TEXT NOT NULL,
    equity          REAL,
    buying_power    REAL,
    cash            REAL,
    portfolio_value REAL,
    raw_json        TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS account_snapshots_time
    ON account_snapshots (captured_at);

CREATE TABLE IF NOT EXISTS position_snapshots (
    id          INTEGER PRIMARY KEY,
    captured_at TEXT NOT NULL,
    symbol      TEXT NOT NULL,
    qty         REAL,
    avg_entry   REAL,
    market_value REAL,
    unrealized_pl REAL,
    raw_json    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS position_snapshots_time
    ON position_snapshots (captured_at);
CREATE INDEX IF NOT EXISTS position_snapshots_symbol
    ON position_snapshots (symbol, captured_at);

CREATE TABLE IF NOT EXISTS orders (
    id          TEXT PRIMARY KEY,
    symbol      TEXT NOT NULL,
    status      TEXT NOT NULL,
    side        TEXT,
    order_type  TEXT,
    qty         REAL,
    filled_qty  REAL,
    updated_at  TEXT NOT NULL,
    raw_json    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS orders_symbol
    ON orders (symbol, updated_at);

CREATE TABLE IF NOT EXISTS quotes (
    symbol     TEXT PRIMARY KEY,
    time       TEXT NOT NULL,
    bid        REAL,
    ask        REAL,
    mid        REAL,
    updated_at TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS backtests (
    id          INTEGER PRIMARY KEY,
    created_at  TEXT NOT NULL,
    symbol      TEXT NOT NULL,
    strategy    TEXT NOT NULL,
    params_json TEXT NOT NULL,
    result_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS backtests_symbol_time
    ON backtests (symbol, created_at);
"#;

/// Persisted shell preferences. Credentials and session data are never here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiState {
    pub symbol: String,
    pub range: String,
    pub blotter_tab: String,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            symbol: "AAPL".to_string(),
            range: "6M".to_string(),
            blotter_tab: "positions".to_string(),
        }
    }
}

impl UiState {
    /// Values come back from disk, so treat them as untrusted input.
    pub fn sanitized(mut self) -> Self {
        let symbol = self.symbol.trim().to_ascii_uppercase();
        let valid_symbol = (1..=10).contains(&symbol.len())
            && symbol
                .chars()
                .next()
                .is_some_and(|first| first.is_ascii_alphabetic())
            && symbol
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '.');
        self.symbol = if valid_symbol {
            symbol
        } else {
            "AAPL".to_string()
        };

        if !matches!(self.range.as_str(), "1D" | "1M" | "6M" | "1Y" | "5Y") {
            self.range = "6M".to_string();
        }

        if !matches!(self.blotter_tab.as_str(), "positions" | "orders") {
            self.blotter_tab = "positions".to_string();
        }

        self
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    pub path: String,
    pub bytes: u64,
    pub bars: i64,
    pub accounts: i64,
    pub positions: i64,
    pub orders: i64,
    pub quotes: i64,
    pub backtests: i64,
}

pub struct Storage {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Storage {
    /// Open or create the database and apply the schema.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AppError> {
        let path = path.into();
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let conn = Connection::open(&path).map_err(db_error)?;
        Self::from_connection(conn, path)
    }

    #[cfg(test)]
    fn open_in_memory() -> Result<Self, AppError> {
        let conn = Connection::open_in_memory().map_err(db_error)?;
        Self::from_connection(conn, PathBuf::from(":memory:"))
    }

    fn from_connection(conn: Connection, path: PathBuf) -> Result<Self, AppError> {
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(db_error)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )
        .map_err(db_error)?;

        let storage = Self {
            conn: Mutex::new(conn),
            path,
        };
        storage.migrate()?;
        Ok(storage)
    }

    fn migrate(&self) -> Result<(), AppError> {
        let conn = self.conn();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(db_error)?;
        if version < SCHEMA_VERSION {
            conn.execute_batch(SCHEMA).map_err(db_error)?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(db_error)?;
        }
        Ok(())
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn ui_state(&self) -> Result<UiState, AppError> {
        let conn = self.conn();
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'ui_state'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?;

        Ok(raw
            .and_then(|json| serde_json::from_str::<UiState>(&json).ok())
            .unwrap_or_default()
            .sanitized())
    }

    pub fn save_ui_state(&self, state: &UiState) -> Result<(), AppError> {
        let clean = state.clone().sanitized();
        let json = serde_json::to_string(&clean).map_err(json_error)?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('ui_state', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )
        .map_err(db_error)?;
        Ok(())
    }

    /// Upsert one request's candles; re-fetching a range is idempotent.
    pub fn save_bars(&self, symbol: &str, timeframe: &str, bars: &[Bar]) -> Result<usize, AppError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(db_error)?;
        {
            let mut statement = tx
                .prepare_cached(
                    "INSERT INTO bars
                        (symbol, timeframe, ts, open, high, low, close, volume)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(symbol, timeframe, ts) DO UPDATE SET
                        open = excluded.open, high = excluded.high,
                        low = excluded.low, close = excluded.close,
                        volume = excluded.volume",
                )
                .map_err(db_error)?;
            for bar in bars {
                statement
                    .execute(params![
                        symbol,
                        timeframe,
                        bar.time,
                        bar.open,
                        bar.high,
                        bar.low,
                        bar.close,
                        bar.volume
                    ])
                    .map_err(db_error)?;
            }
        }
        tx.commit().map_err(db_error)?;
        Ok(bars.len())
    }

    /// Keep one point-in-time account row and one row per open position.
    pub fn record_overview(
        &self,
        account: Option<&Value>,
        positions: Option<&Value>,
        orders: Option<&Value>,
    ) -> Result<(), AppError> {
        let mut conn = self.conn();
        let captured_at = Utc::now().to_rfc3339();
        let tx = conn.transaction().map_err(db_error)?;

        if let Some(account) = account {
            tx.execute(
                "INSERT INTO account_snapshots
                    (captured_at, equity, buying_power, cash, portfolio_value, raw_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    captured_at,
                    number(account, "equity"),
                    number(account, "buying_power"),
                    number(account, "cash"),
                    number(account, "portfolio_value"),
                    account.to_string()
                ],
            )
            .map_err(db_error)?;
        }

        if let Some(Value::Array(rows)) = positions {
            for row in rows {
                let symbol = text(row, "symbol").unwrap_or_default();
                if symbol.is_empty() {
                    continue;
                }
                tx.execute(
                    "INSERT INTO position_snapshots
                        (captured_at, symbol, qty, avg_entry, market_value, unrealized_pl, raw_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        captured_at,
                        symbol,
                        number(row, "qty").or_else(|| number(row, "quantity")),
                        number(row, "avg_entry_price")
                            .or_else(|| number(row, "average_entry_price")),
                        number(row, "market_value"),
                        number(row, "unrealized_pl")
                            .or_else(|| number(row, "unrealized_gain_total")),
                        row.to_string()
                    ],
                )
                .map_err(db_error)?;
            }
        }

        if let Some(Value::Array(rows)) = orders {
            for row in rows {
                upsert_order(&tx, row, &captured_at).map_err(db_error)?;
            }
        }

        tx.commit().map_err(db_error)?;
        Ok(())
    }

    pub fn save_order(&self, order: &Value) -> Result<(), AppError> {
        let conn = self.conn();
        upsert_order(&conn, order, &Utc::now().to_rfc3339()).map_err(db_error)
    }

    pub fn save_quote(&self, quote: &Quote) -> Result<(), AppError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO quotes (symbol, time, bid, ask, mid, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(symbol) DO UPDATE SET
                time = excluded.time, bid = excluded.bid, ask = excluded.ask,
                mid = excluded.mid, updated_at = excluded.updated_at",
            params![
                quote.symbol,
                quote.time,
                quote.bid,
                quote.ask,
                quote.mid,
                Utc::now().to_rfc3339()
            ],
        )
        .map_err(db_error)?;
        Ok(())
    }

    /// Save one completed backtest. The JSON is kept whole so a future UI can
    /// reopen a run without migrating a dozen result columns.
    pub fn record_backtest(
        &self,
        symbol: &str,
        strategy: &str,
        params_json: &str,
        result_json: &str,
    ) -> Result<(), AppError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO backtests (created_at, symbol, strategy, params_json, result_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                Utc::now().to_rfc3339(),
                symbol,
                strategy,
                params_json,
                result_json
            ],
        )
        .map_err(db_error)?;
        Ok(())
    }

    pub fn stats(&self) -> Result<StorageStats, AppError> {
        let conn = self.conn();
        let count = |table: &str| -> Result<i64, AppError> {
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .map_err(db_error)
        };

        let bytes = file_bytes(&self.path)
            + file_bytes(&sibling(&self.path, "-wal"))
            + file_bytes(&sibling(&self.path, "-shm"));

        Ok(StorageStats {
            path: self.path.display().to_string(),
            bytes,
            bars: count("bars")?,
            accounts: count("account_snapshots")?,
            positions: count("position_snapshots")?,
            orders: count("orders")?,
            quotes: count("quotes")?,
            backtests: count("backtests")?,
        })
    }
}

fn upsert_order(conn: &Connection, order: &Value, captured_at: &str) -> rusqlite::Result<()> {
    let id = text(order, "id").unwrap_or_default();
    let symbol = text(order, "symbol").unwrap_or_default();
    if id.is_empty() || symbol.is_empty() {
        return Ok(());
    }

    conn.execute(
        "INSERT INTO orders
            (id, symbol, status, side, order_type, qty, filled_qty, updated_at, raw_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
            symbol = excluded.symbol, status = excluded.status, side = excluded.side,
            order_type = excluded.order_type, qty = excluded.qty,
            filled_qty = excluded.filled_qty, updated_at = excluded.updated_at,
            raw_json = excluded.raw_json",
        params![
            id,
            symbol,
            text(order, "status").unwrap_or_else(|| "unknown".to_string()),
            text(order, "side"),
            text(order, "order_type").or_else(|| text(order, "type")),
            number(order, "qty"),
            number(order, "filled_qty"),
            text(order, "updated_at").unwrap_or_else(|| captured_at.to_string()),
            order.to_string()
        ],
    )?;
    Ok(())
}

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(|field| {
        field
            .as_f64()
            .or_else(|| field.as_str().and_then(|raw| raw.parse().ok()))
    })
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", path.display()))
}

fn file_bytes(path: &Path) -> u64 {
    fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0)
}

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Internal(format!("local storage: {error}"))
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::Internal(format!("local storage: {error}"))
}

fn json_error(error: serde_json::Error) -> AppError {
    AppError::Internal(format!("local storage: {error}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn store() -> Storage {
        Storage::open_in_memory().expect("in-memory database")
    }

    #[test]
    fn ui_state_round_trips_and_sanitizes() {
        let storage = store();
        let saved = UiState {
            symbol: "msft".to_string(),
            range: "1Y".to_string(),
            blotter_tab: "orders".to_string(),
        };

        storage.save_ui_state(&saved).unwrap();
        let loaded = storage.ui_state().unwrap();

        assert_eq!(loaded.symbol, "MSFT");
        assert_eq!(loaded.range, "1Y");
        assert_eq!(loaded.blotter_tab, "orders");

        storage
            .save_ui_state(&UiState {
                symbol: "<bad>".to_string(),
                range: "10Y".to_string(),
                blotter_tab: "wat".to_string(),
            })
            .unwrap();
        let fallback = storage.ui_state().unwrap();
        assert_eq!(fallback.symbol, "AAPL");
        assert_eq!(fallback.range, "6M");
        assert_eq!(fallback.blotter_tab, "positions");
    }

    #[test]
    fn file_database_survives_reopen() {
        let path = std::env::temp_dir().join(format!("traiding-test-{}.db", std::process::id()));
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(sibling(&path, "-wal"));
        let _ = fs::remove_file(sibling(&path, "-shm"));

        let storage = Storage::open(&path).expect("file database");
        storage
            .save_ui_state(&UiState {
                symbol: "TSLA".to_string(),
                range: "1D".to_string(),
                blotter_tab: "orders".to_string(),
            })
            .unwrap();
        drop(storage);

        let reopened = Storage::open(&path).expect("reopen file database");
        assert_eq!(reopened.ui_state().unwrap().symbol, "TSLA");
        assert_eq!(reopened.stats().unwrap().path, path.display().to_string());

        drop(reopened);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(sibling(&path, "-wal"));
        let _ = fs::remove_file(sibling(&path, "-shm"));
    }

    #[test]
    fn ui_state_uses_camel_case_on_the_wire() {
        let value = serde_json::to_value(UiState::default()).unwrap();
        assert_eq!(value["blotterTab"], "positions");
        assert!(
            value.get("blotter_tab").is_none(),
            "ui_state fields are camelCase in IPC and on disk"
        );
    }

    #[test]
    fn bars_are_upserted_not_duplicated() {
        let storage = store();
        let bar = Bar {
            time: 1_700_000_000,
            open: 10.0,
            high: 11.0,
            low: 9.5,
            close: 10.5,
            volume: 1000.0,
        };

        storage.save_bars("AAPL", "1Day", &[bar]).unwrap();
        let updated = Bar {
            time: 1_700_000_000,
            close: 12.0,
            ..bar
        };
        storage.save_bars("AAPL", "1Day", &[updated]).unwrap();

        assert_eq!(storage.stats().unwrap().bars, 1);
    }

    #[test]
    fn backtests_are_counted_in_storage_stats() {
        let storage = store();
        storage
            .record_backtest("AAPL", "SMA crossover", "{}", "{}")
            .unwrap();
        storage
            .record_backtest("MSFT", "SMA crossover", "{}", "{}")
            .unwrap();

        assert_eq!(storage.stats().unwrap().backtests, 2);
    }

    #[test]
    fn overview_writes_one_snapshot_set_and_upserts_orders() {
        let storage = store();
        let account = json!({
            "equity": "100000.00",
            "buying_power": "50000.00",
            "cash": "25000.00",
            "portfolio_value": "100000.00"
        });
        let positions = json!([{
            "symbol": "AAPL",
            "qty": "10",
            "avg_entry_price": "180.00",
            "market_value": "2000.00",
            "unrealized_pl": "200.00"
        }]);
        let orders = json!([{
            "id": "order-1",
            "symbol": "AAPL",
            "status": "new",
            "side": "buy",
            "order_type": "limit",
            "qty": "1"
        }]);

        storage
            .record_overview(Some(&account), Some(&positions), Some(&orders))
            .unwrap();

        let stats = storage.stats().unwrap();
        assert_eq!(stats.accounts, 1);
        assert_eq!(stats.positions, 1);
        assert_eq!(stats.orders, 1);
    }
}
