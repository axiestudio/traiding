//! Alpaca websocket bridges.
//!
//! Market data and order updates are opened inside the Rust process and bridged
//! to the webview as Tauri events. The credentials stay in `Session`; the
//! payloads below contain prices and order JSON only.
//!
//! The market loop subscribes to exactly one symbol at a time. A watch channel
//! carries symbol changes to the loop so switching tickers does not tear down
//! the socket. Both loops reconnect with bounded backoff, and every state
//! transition is reported through `stream:status`.

use std::sync::Arc;
use std::time::Duration;

use apca::api::v2::updates::OrderUpdates;
use apca::data::v2::stream::{self, Data, IEX, MarketData, RealtimeData};
use futures::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::state::Session;

/// Handle for the market feed. Dropping the sender ends the watch channel, but
/// the state always aborts the task explicitly on disconnect.
pub struct MarketStream {
    pub symbol_tx: watch::Sender<String>,
    pub task: tauri::async_runtime::JoinHandle<()>,
}

pub struct OrdersStream {
    pub task: tauri::async_runtime::JoinHandle<()>,
}

const MIN_BACKOFF: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const EVENT_QUOTE: &str = "market:quote";
const EVENT_TRADE: &str = "market:trade";
const EVENT_ORDER: &str = "trade:update";
const EVENT_STATUS: &str = "stream:status";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuoteEvent {
    symbol: String,
    time: String,
    bid: f64,
    ask: f64,
    bid_size: u64,
    ask_size: u64,
    mid: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TradeEvent {
    symbol: String,
    time: String,
    price: f64,
    size: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderEvent {
    event: serde_json::Value,
    order: serde_json::Value,
}

#[derive(Clone, Serialize)]
struct StreamStatus {
    channel: &'static str,
    state: &'static str,
}

fn status(app: &AppHandle, channel: &'static str, state: &'static str) {
    if let Err(err) = app.emit(
        EVENT_STATUS,
        StreamStatus {
            channel,
            state,
        },
    ) {
        debug!(%err, channel, state, "stream status not delivered");
    }
}

fn market_data(symbol: &str) -> MarketData {
    let mut data = MarketData::default();
    data.set_quotes(vec![symbol.to_string()]);
    data.set_trades(vec![symbol.to_string()]);
    data
}

fn number(value: &num_decimal::Num) -> f64 {
    value.to_string().parse().unwrap_or_default()
}

fn whole(value: &num_decimal::Num) -> u64 {
    value
        .to_string()
        .parse::<f64>()
        .ok()
        .filter(|n| *n > 0.0)
        .map(|n| n as u64)
        .unwrap_or(0)
}

/// Market quotes and trades for one symbol, reconnecting forever until aborted.
pub async fn market_loop(
    app: AppHandle,
    session: Arc<Session>,
    mut symbol_rx: watch::Receiver<String>,
) {
    let mut backoff = MIN_BACKOFF;

    loop {
        status(&app, "market", "connecting");
        let mut symbol = symbol_rx.borrow().clone();

        let (mut stream, mut subscription) = match session
            .client
            .subscribe::<RealtimeData<IEX>>()
            .await
        {
            Ok(pair) => pair,
            Err(err) => {
                warn!(%err, "market stream could not connect");
                status(&app, "market", "offline");
                sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };

        let want = market_data(&symbol);
        match stream::drive(Box::pin(subscription.subscribe(&want)), &mut stream).await {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(err))) => {
                warn!(%err, "market subscription rejected");
                status(&app, "market", "offline");
                sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
            Ok(Err(err)) => {
                warn!(%err, "market subscription transport failed");
                status(&app, "market", "offline");
                sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
            Err(message) => {
                warn!(?message, "market stream dropped during subscription");
                status(&app, "market", "offline");
                sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        }

        status(&app, "market", "live");
        backoff = MIN_BACKOFF;

        let mut pending_symbol: Option<String> = None;
        loop {
            tokio::select! {
                changed = symbol_rx.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    let next = symbol_rx.borrow().clone();
                    if next != symbol {
                        pending_symbol = Some(next);
                    }
                }
                message = stream.next() => {
                    match message {
                        Some(Ok(Ok(Data::Quote(quote)))) => {
                            let bid = number(&quote.bid_price);
                            let ask = number(&quote.ask_price);
                            let event = QuoteEvent {
                                symbol: quote.symbol,
                                time: quote.timestamp.to_rfc3339(),
                                bid,
                                ask,
                                bid_size: whole(&quote.bid_size),
                                ask_size: whole(&quote.ask_size),
                                mid: (bid + ask) / 2.0,
                            };
                            if let Err(err) = app.emit(EVENT_QUOTE, event) {
                                debug!(%err, "quote event not delivered");
                            }
                        }
                        Some(Ok(Ok(Data::Trade(trade)))) => {
                            let event = TradeEvent {
                                symbol: trade.symbol,
                                time: trade.timestamp.to_rfc3339(),
                                price: number(&trade.trade_price),
                                size: whole(&trade.trade_size),
                            };
                            if let Err(err) = app.emit(EVENT_TRADE, event) {
                                debug!(%err, "trade event not delivered");
                            }
                        }
                        Some(Ok(Ok(Data::Bar(_)))) => {}
                        Some(Ok(Ok(_))) => {}
                        Some(Ok(Err(err))) => {
                            warn!(%err, "market message could not be parsed");
                        }
                        Some(Err(err)) => {
                            warn!(%err, "market stream closed");
                            break;
                        }
                        None => {
                            warn!("market stream ended");
                            break;
                        }
                    }
                }
            }

            let Some(next) = pending_symbol.take() else {
                continue;
            };

            let old = market_data(&symbol);
            let _ = stream::drive(Box::pin(subscription.unsubscribe(&old)), &mut stream).await;

            let want = market_data(&next);
            match stream::drive(Box::pin(subscription.subscribe(&want)), &mut stream).await {
                Ok(Ok(Ok(()))) => {
                    symbol = next;
                    status(&app, "market", "live");
                }
                _ => {
                    warn!(symbol = %next, "market symbol change rejected");
                    status(&app, "market", "offline");
                    break;
                }
            }
        }

        status(&app, "market", "offline");
        sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

/// Order updates for the current account, reconnecting forever until aborted.
pub async fn orders_loop(app: AppHandle, session: Arc<Session>) {
    let mut backoff = MIN_BACKOFF;

    loop {
        status(&app, "orders", "connecting");

        match session.client.subscribe::<OrderUpdates>().await {
            Ok((mut stream, _subscription)) => {
                status(&app, "orders", "live");
                backoff = MIN_BACKOFF;

                while let Some(message) = stream.next().await {
                    match message {
                        Ok(Ok(update)) => {
                            let event = OrderEvent {
                                event: serde_json::to_value(update.event)
                                    .unwrap_or(serde_json::Value::Null),
                                order: serde_json::to_value(&update.order)
                                    .unwrap_or(serde_json::Value::Null),
                            };
                            if let Err(err) = app.emit(EVENT_ORDER, event) {
                                debug!(%err, "order event not delivered");
                            }
                        }
                        Ok(Err(err)) => {
                            warn!(%err, "order update could not be parsed");
                        }
                        Err(err) => {
                            warn!(%err, "order stream closed");
                            break;
                        }
                    }
                }
            }
            Err(err) => {
                warn!(%err, "order stream could not connect");
            }
        }

        status(&app, "orders", "offline");
        sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}
