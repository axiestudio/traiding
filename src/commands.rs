//! IPC commands — the entire backend contract.
//!
//! These replace the old axum handlers one for one. Each command resolves the
//! [`Session`] from state, issues one `apca` request, and hands back the SDK's
//! own serialized types where possible, so the UI contract stays aligned with
//! Alpaca's docs instead of inventing a parallel schema.
//!
//! Commands return `Result<T, String>`; the `String` is already a clean,
//! human readable sentence prepared by [`crate::error`].

use std::sync::Arc;

use chrono::{Duration, Utc};
use num_decimal::Num;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, State};

use apca::api::v2::{account, asset, assets, clock, order, orders, position, positions};
use apca::data::v2::bars::{self, Adjustment, TimeFrame};
use apca::data::v2::last_quotes;
use apca::data::v2::Feed;

use crate::error::{bad_request, from_request_error, AppError};
use crate::state::{AssetRow, AppState, Session};
use crate::storage::{StorageStats, UiState};

type Ipc<T> = Result<T, String>;

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectReq {
    pub key_id: String,
    pub secret: String,
    /// `paper` (default) or `live`.
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Serialize)]
pub struct SessionInfo {
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    pub paper: bool,
}

impl SessionInfo {
    fn disconnected() -> Self {
        Self {
            connected: false,
            key_id: None,
            base_url: None,
            paper: true,
        }
    }

    fn from_session(session: &Session) -> Self {
        Self {
            connected: true,
            key_id: Some(session.masked_key()),
            base_url: Some(session.base_url.clone()),
            paper: session.paper,
        }
    }
}

#[tauri::command]
pub async fn get_session(state: State<'_, Arc<AppState>>) -> Ipc<SessionInfo> {
    Ok(match state.session() {
        Ok(session) => SessionInfo::from_session(&session),
        Err(_) => SessionInfo::disconnected(),
    })
}

#[tauri::command]
pub async fn connect(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
    req: ConnectReq,
) -> Ipc<SessionInfo> {
    let key_id = req.key_id.trim().to_string();
    let secret = req.secret.trim().to_string();

    if key_id.is_empty() || secret.is_empty() {
        return Err(bad_request("both an API key ID and a secret are required").into());
    }

    // The connect form shows a Key ID, a Secret, and mentions an endpoint URL,
    // so people paste the URL into one of the credential boxes. Catch it here
    // rather than shipping a nonsense header to Alpaca.
    for (field, value) in [("key ID", &key_id), ("secret", &secret)] {
        if value.contains("://") || value.contains("alpaca.markets") {
            return Err(bad_request(format!(
                "that looks like the API endpoint, not the {field} — paste the value \
                 labelled \"Key\" into Key ID and the secret shown when the key was created"
            ))
            .into());
        }
    }

    // A real secret is ~40 characters. Catching the obvious mismatch early
    // gives a much clearer message than a bare upstream `unauthorized.`
    if secret.len() < 20 {
        return Err(bad_request(
            "the secret looks too short — Alpaca only displays it once, when the key is \
             created; use Regenerate in the dashboard to get a fresh pair",
        )
        .into());
    }

    let paper = match req.mode.as_deref().unwrap_or("paper") {
        "live" => false,
        "paper" => true,
        other => return Err(bad_request(format!("unknown mode `{other}`")).into()),
    };

    let base_url = if paper {
        crate::state::PAPER_URL
    } else {
        crate::state::LIVE_URL
    }
    .to_string();

    let session = Session::connect(key_id, secret, base_url, paper).await?;
    let info = SessionInfo::from_session(&session);

    state.set_session(session);
    state.start_order_stream(app);
    Ok(info)
}

#[tauri::command]
pub async fn disconnect(state: State<'_, Arc<AppState>>) -> Ipc<SessionInfo> {
    state.clear_session();
    Ok(SessionInfo::disconnected())
}

// ---------------------------------------------------------------------------
// Account & clock
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_account(state: State<'_, Arc<AppState>>) -> Ipc<Value> {
    let session = state.session()?;
    let account = session
        .client
        .issue::<account::Get>(&())
        .await
        .map_err(from_request_error)?;
    Ok(serde_json::to_value(account).unwrap_or(Value::Null))
}

#[tauri::command]
pub async fn get_clock(state: State<'_, Arc<AppState>>) -> Ipc<Value> {
    let session = state.session()?;
    let clock = session
        .client
        .issue::<clock::Get>(&())
        .await
        .map_err(from_request_error)?;
    Ok(serde_json::to_value(clock).unwrap_or(Value::Null))
}

// ---------------------------------------------------------------------------
// Positions
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_positions(state: State<'_, Arc<AppState>>) -> Ipc<Value> {
    let session = state.session()?;
    let positions = session
        .client
        .issue::<positions::List>(&())
        .await
        .map_err(from_request_error)?;
    Ok(serde_json::to_value(positions).unwrap_or(Value::Null))
}

#[tauri::command]
pub async fn close_position(state: State<'_, Arc<AppState>>, symbol: String) -> Ipc<Value> {
    let session = state.session()?;
    let symbol = parse_symbol(&symbol)?;
    let order = session
        .client
        .issue::<position::Delete>(&symbol)
        .await
        .map_err(from_request_error)?;
    Ok(serde_json::to_value(order).unwrap_or(Value::Null))
}

// ---------------------------------------------------------------------------
// Orders
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_orders(
    state: State<'_, Arc<AppState>>,
    status: Option<String>,
    limit: Option<usize>,
) -> Ipc<Value> {
    let session = state.session()?;

    let status = match status.as_deref().unwrap_or("all") {
        "open" => orders::Status::Open,
        "closed" => orders::Status::Closed,
        "all" => orders::Status::All,
        other => return Err(bad_request(format!("unknown order status `{other}`")).into()),
    };

    let req = orders::ListReq {
        status,
        limit: Some(limit.unwrap_or(100).clamp(1, 500)),
        ..Default::default()
    };

    let orders = session
        .client
        .issue::<orders::List>(&req)
        .await
        .map_err(from_request_error)?;
    Ok(serde_json::to_value(orders).unwrap_or(Value::Null))
}

/// Order ticket submitted by the UI.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOrderReq {
    pub symbol: String,
    pub side: String,
    #[serde(default = "default_order_type")]
    pub r#type: String,
    #[serde(default)]
    pub qty: Option<f64>,
    #[serde(default)]
    pub notional: Option<f64>,
    #[serde(default)]
    pub limit_price: Option<f64>,
    #[serde(default)]
    pub stop_price: Option<f64>,
    #[serde(default = "default_tif")]
    pub time_in_force: String,
    #[serde(default)]
    pub extended_hours: bool,
}

fn default_order_type() -> String {
    "market".to_string()
}

fn default_tif() -> String {
    "day".to_string()
}

#[tauri::command]
pub async fn create_order(state: State<'_, Arc<AppState>>, req: CreateOrderReq) -> Ipc<Value> {
    let session = state.session()?;

    let symbol = normalize_symbol(&req.symbol)?;

    let side = match req.side.to_ascii_lowercase().as_str() {
        "buy" => order::Side::Buy,
        "sell" => order::Side::Sell,
        other => return Err(bad_request(format!("unknown side `{other}`")).into()),
    };

    let type_ = match req.r#type.to_ascii_lowercase().as_str() {
        "market" => order::Type::Market,
        "limit" => order::Type::Limit,
        "stop" => order::Type::Stop,
        "stop_limit" => order::Type::StopLimit,
        other => return Err(bad_request(format!("unsupported order type `{other}`")).into()),
    };

    let time_in_force = match req.time_in_force.to_ascii_lowercase().as_str() {
        "day" => order::TimeInForce::Day,
        "gtc" => order::TimeInForce::UntilCanceled,
        "ioc" => order::TimeInForce::ImmediateOrCancel,
        "fok" => order::TimeInForce::FillOrKill,
        other => {
            return Err(bad_request(format!("unsupported time in force `{other}`")).into());
        }
    };

    // Alpaca requires exactly one of qty / notional.
    let amount = match (req.qty, req.notional) {
        (Some(_), Some(_)) => {
            return Err(bad_request("provide either a quantity or a notional, not both").into());
        }
        (None, None) => return Err(bad_request("a quantity or a notional is required").into()),
        (Some(qty), None) => {
            if qty <= 0.0 {
                return Err(bad_request("quantity must be greater than zero").into());
            }
            order::Amount::Quantity {
                quantity: num_from_f64(qty)?,
            }
        }
        (None, Some(notional)) => {
            if notional <= 0.0 {
                return Err(bad_request("notional must be greater than zero").into());
            }
            order::Amount::Notional {
                notional: num_from_f64(notional)?,
            }
        }
    };

    let limit_price = match req.limit_price {
        Some(price) => Some(num_from_f64(price)?),
        None => None,
    };
    let stop_price = match req.stop_price {
        Some(price) => Some(num_from_f64(price)?),
        None => None,
    };

    if matches!(type_, order::Type::Limit | order::Type::StopLimit) && limit_price.is_none() {
        return Err(bad_request("a limit order needs a limit price").into());
    }
    if matches!(type_, order::Type::Stop | order::Type::StopLimit) && stop_price.is_none() {
        return Err(bad_request("a stop order needs a stop price").into());
    }

    let create = order::CreateReqInit {
        type_,
        time_in_force,
        limit_price,
        stop_price,
        extended_hours: req.extended_hours,
        ..Default::default()
    }
    .init(symbol, side, amount);

    let created = session
        .client
        .issue::<order::Create>(&create)
        .await
        .map_err(from_request_error)?;
    let created = serde_json::to_value(created).unwrap_or(Value::Null);

    // Best-effort local copy so the order shows in history immediately, even
    // before the next dashboard poll.
    if let Some(storage) = state.storage()
        && let Err(err) = storage.save_order(&created)
    {
        tracing::warn!(%err, "order not stored locally");
    }

    Ok(created)
}

#[tauri::command]
pub async fn cancel_order(state: State<'_, Arc<AppState>>, id: String) -> Ipc<Value> {
    let session = state.session()?;

    let uuid =
        uuid::Uuid::parse_str(&id).map_err(|_| bad_request("order id must be a UUID"))?;
    let id = order::Id(uuid);

    session
        .client
        .issue::<order::Delete>(&id)
        .await
        .map_err(from_request_error)?;

    Ok(json!({ "canceled": id.as_simple().to_string() }))
}

// ---------------------------------------------------------------------------
// Assets
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct AssetsPage {
    pub total: usize,
    pub count: usize,
    pub assets: Vec<AssetRow>,
}

#[tauri::command]
pub async fn list_assets(
    state: State<'_, Arc<AppState>>,
    q: Option<String>,
    limit: Option<usize>,
    refresh: Option<bool>,
) -> Ipc<AssetsPage> {
    let session = state.session()?;

    let rows = match state.cached_assets() {
        Some(rows) if !refresh.unwrap_or(false) => rows,
        _ => {
            let req = assets::ListReq {
                status: asset::Status::Active,
                class: asset::Class::UsEquity,
                ..Default::default()
            };
            let fetched = session
                .client
                .issue::<assets::List>(&req)
                .await
                .map_err(from_request_error)?;

            let rows: Vec<AssetRow> = fetched
                .into_iter()
                .map(|asset| AssetRow {
                    symbol: asset.symbol,
                    exchange: asset.exchange.as_ref().to_string(),
                    class: asset.class.as_ref().to_string(),
                    tradable: asset.tradable,
                    fractionable: asset.fractionable,
                    shortable: asset.shortable,
                    easy_to_borrow: asset.easy_to_borrow,
                })
                .collect();

            state.store_assets(rows.clone());
            rows
        }
    };

    let needle = q.as_deref().unwrap_or("").trim().to_ascii_uppercase();
    let limit = limit.unwrap_or(25).clamp(1, 200);

    let matches: Vec<AssetRow> = rows
        .iter()
        .filter(|row| row.tradable)
        .filter(|row| needle.is_empty() || row.symbol.contains(&needle))
        .take(limit)
        .cloned()
        .collect();

    Ok(AssetsPage {
        total: rows.len(),
        count: matches.len(),
        assets: matches,
    })
}

#[tauri::command]
pub async fn get_asset(state: State<'_, Arc<AppState>>, symbol: String) -> Ipc<Value> {
    let session = state.session()?;
    let symbol = parse_symbol(&symbol)?;
    let asset = session
        .client
        .issue::<asset::Get>(&symbol)
        .await
        .map_err(from_request_error)?;
    Ok(serde_json::to_value(asset).unwrap_or(Value::Null))
}

// ---------------------------------------------------------------------------
// Market data
// ---------------------------------------------------------------------------

/// One OHLCV candle. `time` is a Unix timestamp in **seconds**, which is the
/// shape Lightweight Charts wants for intraday and daily data alike.
#[derive(Serialize, Clone, Copy)]
pub struct Bar {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Serialize)]
pub struct BarSeries {
    pub symbol: String,
    pub bars: Vec<Bar>,
    pub next_page_token: Option<String>,
}

#[tauri::command]
pub async fn get_bars(
    state: State<'_, Arc<AppState>>,
    symbol: String,
    timeframe: Option<String>,
    days: Option<i64>,
    limit: Option<usize>,
) -> Ipc<BarSeries> {
    let session = state.session()?;
    let symbol = symbol.to_ascii_uppercase();

    let (timeframe, timeframe_key) = match timeframe.as_deref().unwrap_or("1Day") {
        "1Min" | "1min" => (TimeFrame::OneMinute, "1Min"),
        "1Hour" | "1hour" => (TimeFrame::OneHour, "1Hour"),
        "1Day" | "1day" => (TimeFrame::OneDay, "1Day"),
        other => return Err(bad_request(format!("unsupported timeframe `{other}`")).into()),
    };

    let days = days.unwrap_or(120).clamp(1, 3650);
    let end = Utc::now();
    let start = end - Duration::days(days);

    let req = bars::ListReqInit {
        limit: Some(limit.unwrap_or(10_000).clamp(1, 10_000)),
        adjustment: Some(Adjustment::All),
        // IEX works on every plan; SIP is rejected on free accounts for recent data.
        feed: Some(Feed::IEX),
        ..Default::default()
    }
    .init(&symbol, start, end, timeframe);

    let (symbol, rows, next_page_token) = fetch_bars_pages(&session.client, req, 20).await?;

    if let Some(storage) = state.storage()
        && let Err(err) = storage.save_bars(&symbol, timeframe_key, &rows)
    {
        tracing::warn!(%err, "bars not stored locally");
    }

    Ok(BarSeries {
        symbol,
        bars: rows,
        next_page_token,
    })
}

/// Follow `next_page_token` until the range is complete or the page cap is hit.
/// Alpaca caps a single bars response, so a 5Y chart needs more than one page.
async fn fetch_bars_pages(
    client: &apca::Client,
    mut req: bars::ListReq,
    max_pages: usize,
) -> Result<(String, Vec<Bar>, Option<String>), AppError> {
    let mut symbol = String::new();
    let mut rows = Vec::new();

    for page in 0..max_pages {
        let response = client
            .issue::<bars::List>(&req)
            .await
            .map_err(from_request_error)?;

        symbol = response.symbol;
        rows.extend(response.bars.iter().map(bar_from));

        match response.next_page_token {
            Some(token) if page + 1 < max_pages => req.page_token = Some(token),
            next => return Ok((symbol, rows, next)),
        }
    }

    Ok((symbol, rows, None))
}

/// Daily bars for the backtester. Kept beside `get_bars` so both use the same
/// adjustment and timestamp conversion.
pub(crate) async fn daily_bars(
    session: &Session,
    symbol: &str,
    days: i64,
) -> Result<Vec<Bar>, AppError> {
    let end = Utc::now();
    let start = end - Duration::days(days.clamp(1, 3650));

    let req = bars::ListReqInit {
        limit: Some(10_000),
        adjustment: Some(Adjustment::All),
        feed: Some(Feed::IEX),
        ..Default::default()
    }
    .init(symbol, start, end, TimeFrame::OneDay);


    let (_, rows, _) = fetch_bars_pages(&session.client, req, 20).await?;
    Ok(rows)
}

fn bar_from(bar: &bars::Bar) -> Bar {
    Bar {
        time: bar.time.timestamp(),
        open: decimal_to_f64(&bar.open),
        high: decimal_to_f64(&bar.high),
        low: decimal_to_f64(&bar.low),
        close: decimal_to_f64(&bar.close),
        volume: bar.volume as f64,
    }
}

#[derive(Serialize)]
pub struct Quote {
    pub symbol: String,
    pub time: String,
    pub bid: f64,
    pub ask: f64,
    pub bid_size: u64,
    pub ask_size: u64,
    pub mid: f64,
}

/// Point the websocket feed at the symbol currently shown in the chart.
///
/// Called on connect and on every symbol change. Without a session this is a
/// harmless no-op so the UI can fire it while the connect dialog is open.
#[tauri::command]
pub async fn set_stream_symbol(
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
    symbol: String,
) -> Ipc<()> {
    if state.session().is_err() {
        return Ok(());
    }
    let symbol = normalize_symbol(&symbol)?;
    state.start_market_stream(app, symbol);
    Ok(())
}

#[tauri::command]
pub async fn get_quote(state: State<'_, Arc<AppState>>, symbol: String) -> Ipc<Quote> {
    let session = state.session()?;
    let symbol = symbol.to_ascii_uppercase();

    let req = last_quotes::GetReqInit::default().init([symbol.clone()]);
    let quotes = session
        .client
        .issue::<last_quotes::Get>(&req)
        .await
        .map_err(from_request_error)?;

    match quotes.into_iter().next() {
        Some((symbol, quote)) => {
            let quote = Quote {
                symbol,
                time: quote.time.to_rfc3339(),
                bid: decimal_to_f64(&quote.bid_price),
                ask: decimal_to_f64(&quote.ask_price),
                bid_size: quote.bid_size,
                ask_size: quote.ask_size,
                mid: decimal_to_f64(
                    &((quote.bid_price.clone() + quote.ask_price.clone()) / Num::from(2)),
                ),
            };

            if let Some(storage) = state.storage()
                && let Err(err) = storage.save_quote(&quote)
            {
                tracing::warn!(%err, "quote not stored locally");
            }

            Ok(quote)
        }
        None => Err(AppError::Upstream(format!("no quote returned for {symbol}")).into()),
    }
}

// ---------------------------------------------------------------------------
// Aggregated dashboard payload
// ---------------------------------------------------------------------------

/// A section that either carries data or its own error, so one upstream hiccup
/// does not blank out the whole dashboard.
#[derive(Serialize)]
pub struct Section {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Section {
    fn data(value: Value) -> Self {
        Self {
            ok: true,
            data: Some(value),
            error: None,
        }
    }

    fn error(message: String) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(message),
        }
    }
}

#[derive(Serialize)]
pub struct Overview {
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paper: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<Section>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock: Option<Section>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub positions: Option<Section>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orders: Option<Section>,
}

#[tauri::command]
pub async fn get_overview(state: State<'_, Arc<AppState>>) -> Ipc<Overview> {
    let session = match state.session() {
        Ok(session) => session,
        Err(_) => {
            return Ok(Overview {
                connected: false,
                key_id: None,
                paper: None,
                base_url: None,
                account: None,
                clock: None,
                positions: None,
                orders: None,
            });
        }
    };

    let (account, clock, positions, orders) = tokio::join!(
        session.client.issue::<account::Get>(&()),
        session.client.issue::<clock::Get>(&()),
        session.client.issue::<positions::List>(&()),
        session.client.issue::<orders::List>(&order_list_req()),
    );

    fn section<T: Serialize, E: std::fmt::Display>(result: Result<T, apca::RequestError<E>>) -> Section {
        match result {
            Ok(value) => Section::data(serde_json::to_value(value).unwrap_or(Value::Null)),
            Err(err) => Section::error(from_request_error(err).to_string()),
        }
    }

    let account = section(account);
    let clock = section(clock);
    let positions = section(positions);
    let orders = section(orders);

    // One row per poll gives a local equity curve and position history without
    // a separate scheduler. Failures must never break the dashboard response.
    if let Some(storage) = state.storage()
        && let Err(err) = storage.record_overview(
            account.data.as_ref(),
            positions.data.as_ref(),
            orders.data.as_ref(),
        )
    {
        tracing::warn!(%err, "dashboard snapshot not stored locally");
    }

    Ok(Overview {
        connected: true,
        key_id: Some(session.masked_key()),
        paper: Some(session.paper),
        base_url: Some(session.base_url.clone()),
        account: Some(account),
        clock: Some(clock),
        positions: Some(positions),
        orders: Some(orders),
    })
}

fn order_list_req() -> orders::ListReq {
    orders::ListReq {
        status: orders::Status::All,
        limit: Some(100),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Local storage
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_ui_state(state: State<'_, Arc<AppState>>) -> Ipc<UiState> {
    let storage = state
        .storage()
        .ok_or_else(|| "local storage is unavailable".to_string())?;
    storage.ui_state().map_err(String::from)
}

#[tauri::command]
pub async fn save_ui_state(
    state: State<'_, Arc<AppState>>,
    req: UiState,
) -> Ipc<()> {
    let storage = state
        .storage()
        .ok_or_else(|| "local storage is unavailable".to_string())?;
    storage.save_ui_state(&req).map_err(String::from)
}

#[tauri::command]
pub async fn get_storage_info(state: State<'_, Arc<AppState>>) -> Ipc<StorageStats> {
    let storage = state
        .storage()
        .ok_or_else(|| "local storage is unavailable".to_string())?;
    storage.stats().map_err(String::from)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `apca::api::v2::asset::Symbol` only accepts upper case alphabetic tickers
/// (or a UUID), so normalise and give a clearer error than the SDK's.
pub(crate) fn normalize_symbol(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim().to_ascii_uppercase();
    if trimmed.is_empty() {
        return Err(bad_request("a symbol is required"));
    }
    trimmed
        .parse::<asset::Symbol>()
        .map_err(|_| bad_request(format!("`{raw}` is not a valid US equity symbol")))
        .map(|symbol| symbol.to_string())
}

fn parse_symbol(raw: &str) -> Result<asset::Symbol, AppError> {
    normalize_symbol(raw).map(asset::Symbol::Sym)
}

fn num_from_f64(value: f64) -> Result<Num, AppError> {
    if !value.is_finite() {
        return Err(bad_request("prices must be finite numbers"));
    }
    // `Num`'s `Display` is the lossless decimal string it was parsed from, so
    // round-tripping through text avoids binary float artefacts like
    // 0.30000000000000004 being sent upstream.
    value
        .to_string()
        .parse::<Num>()
        .map_err(|_| bad_request(format!("`{value}` is not a valid number")))
}

/// `Num` renders the exact decimal it holds; parsing that back into `f64` is
/// only for display, never for a value we send upstream.
fn decimal_to_f64(value: &Num) -> f64 {
    value.to_string().parse::<f64>().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The webview and the Rust structs are two independently edited sides of one
    // contract, so these tests pin the exact JSON `ui/src/api.ts` sends. When the
    // connect form failed with "missing field `keyId`" it was because the UI sent
    // `key_id` while this struct asked for camelCase.
    #[test]
    fn connect_req_accepts_the_payload_the_ui_sends() {
        let req: ConnectReq = serde_json::from_str(
            r#"{"keyId":"PKTEST1234","secret":"somesecretvalue123456","mode":"paper"}"#,
        )
        .expect("connect payload from ui/src/api.ts must deserialize");

        assert_eq!(req.key_id, "PKTEST1234");
        assert_eq!(req.secret, "somesecretvalue123456");
        assert_eq!(req.mode.as_deref(), Some("paper"));
    }

    #[test]
    fn connect_req_rejects_snake_case_key_id() {
        // Guards against quietly "fixing" the UI back to snake_case without
        // changing the struct: the field name is camelCase on the wire.
        let parsed = serde_json::from_str::<ConnectReq>(
            r#"{"key_id":"PKTEST1234","secret":"somesecretvalue123456"}"#,
        );
        assert!(parsed.is_err(), "key_id must not be accepted over IPC");
    }

    #[test]
    fn create_order_req_accepts_the_payload_the_ui_sends() {
        let req: CreateOrderReq = serde_json::from_str(
            r#"{"symbol":"AAPL","side":"buy","type":"limit","qty":3,
                "limitPrice":188.5,"timeInForce":"day","extendedHours":false}"#,
        )
        .expect("order payload from ui/src/api.ts must deserialize");

        assert_eq!(req.symbol, "AAPL");
        assert_eq!(req.r#type, "limit");
        assert_eq!(req.qty, Some(3.0));
        assert_eq!(req.limit_price, Some(188.5));
        assert_eq!(req.time_in_force, "day");
    }

    #[test]
    fn create_order_req_defaults_type_and_tif_when_omitted() {
        let req: CreateOrderReq =
            serde_json::from_str(r#"{"symbol":"AAPL","side":"sell","qty":1}"#).unwrap();

        assert_eq!(req.r#type, "market");
        assert_eq!(req.time_in_force, "day");
        assert!(!req.extended_hours);
    }
}
