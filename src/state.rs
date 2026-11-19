//! Shared application state: the Alpaca session plus small caches.

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use apca::{ApiInfo, Client};
use tauri::AppHandle;
use tokio::sync::watch;

use crate::error::AppError;
use crate::storage::Storage;
use crate::stream::{self, MarketStream, OrdersStream};

/// Public base URLs. Alpaca keeps paper and live trading on separate hosts.
pub const PAPER_URL: &str = "https://paper-api.alpaca.markets";
pub const LIVE_URL: &str = "https://api.alpaca.markets";

const ASSET_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Infer which environment a key belongs to from its prefix.
///
/// Alpaca issues paper keys with a `PK` prefix and live keys with `AK`, and a
/// key is only valid on its own host: sending a live key to the paper endpoint
/// (or the reverse) always fails with a bare `unauthorized.`. Returns `None`
/// for anything that does not follow the convention, so callers can fall back
/// to an explicit choice.
pub fn infer_paper(key_id: &str) -> Option<bool> {
    let key = key_id.trim();
    if key.starts_with("PK") {
        Some(true)
    } else if key.starts_with("AK") {
        Some(false)
    } else {
        None
    }
}

/// A validated set of credentials plus the client built from them.
pub struct Session {
    pub client: Arc<Client>,
    pub key_id: String,
    pub base_url: String,
    pub paper: bool,
}

impl Session {
    /// Validate credentials by issuing a cheap authenticated request.
    ///
    /// Keeping the client behind an `Arc` means every request reuses the same
    /// connection pool instead of rebuilding a TLS stack, which matters once
    /// the dashboard starts polling.
    pub async fn connect(
        key_id: String,
        secret: String,
        base_url: String,
        paper: bool,
    ) -> Result<Self, AppError> {
        // A live key on the paper host (or vice versa) always fails upstream
        // with a bare `unauthorized.`, which reads like a typo in the secret.
        // The prefix makes the real problem detectable, so say so directly.
        if let Some(key_paper) = infer_paper(&key_id)
            && key_paper != paper
        {
            let (key_kind, chosen, fix) = if key_paper {
                ("paper", "live", "switch the mode to Paper")
            } else {
                ("live", "paper", "switch the mode to Live")
            };
            return Err(AppError::BadRequest(format!(
                "that is a {key_kind}-trading key but the {chosen} endpoint was selected — {fix}, \
                 or paste the matching key pair"
            )));
        }

        let api_info = ApiInfo::from_parts(&base_url, &key_id, &secret)
            .map_err(|e| AppError::BadRequest(format!("invalid API base URL: {e}")))?;
        let client = Arc::new(Client::new(api_info));

        client
            .issue::<apca::api::v2::account::Get>(&())
            .await
            .map_err(|e| {
                crate::error::from_request_error(e)
            })?;

        Ok(Self {
            client,
            key_id,
            base_url,
            paper,
        })
    }

    /// `PKTESTABCDEF123456` -> `PKTE…3456`
    pub fn masked_key(&self) -> String {
        let key = &self.key_id;
        if key.len() <= 8 {
            return "…".to_string();
        }
        format!("{}…{}", &key[..4], &key[key.len() - 4..])
    }
}

/// One searchable asset, flattened for the JSON API.
#[derive(Clone, Debug, serde::Serialize)]
pub struct AssetRow {
    pub symbol: String,
    pub exchange: String,
    pub class: String,
    pub tradable: bool,
    pub fractionable: bool,
    pub shortable: bool,
    pub easy_to_borrow: bool,
}

struct AssetsCache {
    rows: Vec<AssetRow>,
    fetched_at: Instant,
}

#[derive(Default)]
struct Inner {
    session: Option<Arc<Session>>,
    assets: Option<AssetsCache>,
    storage: Option<Arc<Storage>>,
    market_stream: Option<MarketStream>,
    order_stream: Option<OrdersStream>,
}

#[derive(Default)]
pub struct AppState {
    inner: RwLock<Inner>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach the local database once, during setup. Survives disconnect so
    /// preferences and history outlive a session.
    pub fn set_storage(&self, storage: Storage) {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        inner.storage = Some(Arc::new(storage));
    }

    pub fn storage(&self) -> Option<Arc<Storage>> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.storage.clone()
    }

    pub fn set_session(&self, session: Session) {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        stop_streams(&mut inner);
        inner.session = Some(Arc::new(session));
        // The asset list is account agnostic, but invalidating on reconnect
        // avoids ever serving data fetched under a previous session.
        inner.assets = None;
    }

    pub fn clear_session(&self) {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        stop_streams(&mut inner);
        inner.session = None;
        inner.assets = None;
    }

    /// Start the quote/trade feed, or repoint it at a new symbol.
    ///
    /// Idempotent and quiet without a session: the UI calls this on every
    /// connect and symbol change, including while disconnected.
    pub fn start_market_stream(&self, app: AppHandle, symbol: String) {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());

        if let Some(existing) = inner.market_stream.as_ref()
            && existing.symbol_tx.send(symbol.clone()).is_ok()
        {
            return;
        }

        stop_market_stream(&mut inner);

        let Some(session) = inner.session.clone() else {
            return;
        };

        let (symbol_tx, symbol_rx) = watch::channel(symbol);
        let task = tauri::async_runtime::spawn(stream::market_loop(
            app,
            session,
            symbol_rx,
        ));
        inner.market_stream = Some(MarketStream { symbol_tx, task });
    }

    /// Start listening for order updates for the current session.
    pub fn start_order_stream(&self, app: AppHandle) {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if inner.order_stream.is_some() {
            return;
        }
        let Some(session) = inner.session.clone() else {
            return;
        };
        let task = tauri::async_runtime::spawn(stream::orders_loop(app, session));
        inner.order_stream = Some(OrdersStream { task });
    }

    pub fn session(&self) -> Result<Arc<Session>, AppError> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.session.clone().ok_or_else(|| {
            AppError::Unauthorized("not connected to Alpaca yet".to_string())
        })
    }

    pub fn cached_assets(&self) -> Option<Vec<AssetRow>> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner
            .assets
            .as_ref()
            .filter(|cache| cache.fetched_at.elapsed() < ASSET_CACHE_TTL)
            .map(|cache| cache.rows.clone())
    }

    pub fn store_assets(&self, rows: Vec<AssetRow>) {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        inner.assets = Some(AssetsCache {
            rows,
            fetched_at: Instant::now(),
        });
    }
}

fn stop_market_stream(inner: &mut Inner) {
    if let Some(handle) = inner.market_stream.take() {
        handle.task.abort();
    }
}

fn stop_streams(inner: &mut Inner) {
    stop_market_stream(inner);
    if let Some(handle) = inner.order_stream.take() {
        handle.task.abort();
    }
}
