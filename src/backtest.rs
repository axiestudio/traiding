//! Long-only SMA-crossover backtester.
//!
//! The engine is deliberately small and deterministic: it reads daily bars,
//! executes a signal on the next bar's open, and marks the position to each
//! close. It never touches the network itself; `run_backtest` fetches the bars
//! through the same `apca` client as the chart.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::State;
use std::sync::Arc;

use crate::commands::{daily_bars, normalize_symbol, Bar};
use crate::error::{bad_request, AppError};
use crate::state::AppState;

type Ipc<T> = Result<T, String>;

const STRATEGY: &str = "SMA crossover";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestReq {
    pub symbol: String,
    #[serde(default)]
    pub days: Option<i64>,
    #[serde(default)]
    pub fast: Option<u32>,
    #[serde(default)]
    pub slow: Option<u32>,
    #[serde(default)]
    pub initial_cash: Option<f64>,
    #[serde(default)]
    pub fee_bps: Option<f64>,
}

struct Params {
    symbol: String,
    days: i64,
    fast: usize,
    slow: usize,
    initial_cash: f64,
    fee_bps: f64,
}

impl BacktestReq {
    fn validate(self) -> Result<Params, AppError> {
        let symbol = normalize_symbol(&self.symbol)?;
        let days = self.days.unwrap_or(365);
        let fast = self.fast.unwrap_or(20);
        let slow = self.slow.unwrap_or(50);
        let initial_cash = self.initial_cash.unwrap_or(10_000.0);
        let fee_bps = self.fee_bps.unwrap_or(0.0);

        if !(60..=3650).contains(&days) {
            return Err(bad_request("days must be between 60 and 3650"));
        }
        if !(2..=200).contains(&fast) {
            return Err(bad_request("fast SMA must be between 2 and 200"));
        }
        if !(3..=300).contains(&slow) {
            return Err(bad_request("slow SMA must be between 3 and 300"));
        }
        if fast >= slow {
            return Err(bad_request("fast SMA must be less than slow SMA"));
        }
        if !initial_cash.is_finite() || initial_cash <= 0.0 {
            return Err(bad_request("starting cash must be greater than zero"));
        }
        if !fee_bps.is_finite() || !(0.0..=100.0).contains(&fee_bps) {
            return Err(bad_request("fee must be between 0 and 100 basis points"));
        }

        Ok(Params {
            symbol,
            days,
            fast: fast as usize,
            slow: slow as usize,
            initial_cash,
            fee_bps,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquityPoint {
    pub time: i64,
    pub equity: f64,
    pub benchmark: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedTrade {
    pub entry_time: i64,
    pub exit_time: i64,
    pub entry_price: f64,
    pub exit_price: f64,
    pub qty: f64,
    pub pnl: f64,
    pub return_pct: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPosition {
    pub entry_time: i64,
    pub entry_price: f64,
    pub qty: f64,
    pub current_price: f64,
    pub unrealized_pnl: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestResult {
    pub symbol: String,
    pub strategy: String,
    pub fast: usize,
    pub slow: usize,
    pub days: i64,
    pub start: String,
    pub end: String,
    pub initial_cash: f64,
    pub final_equity: f64,
    pub total_return_pct: f64,
    pub benchmark_return_pct: f64,
    pub max_drawdown_pct: f64,
    pub win_rate_pct: f64,
    pub trades: usize,
    pub wins: usize,
    pub equity_curve: Vec<EquityPoint>,
    pub closed_trades: Vec<ClosedTrade>,
    pub open_position: Option<OpenPosition>,
}

struct Entry {
    time: i64,
    price: f64,
    qty: f64,
    cash_before: f64,
}

#[derive(Clone, Copy)]
enum Action {
    Buy,
    Sell,
}

fn sma_series(bars: &[Bar], window: usize) -> Vec<Option<f64>> {
    let mut out = vec![None; bars.len()];
    if window == 0 || window > bars.len() {
        return out;
    }

    let mut sum = 0.0;
    for (index, bar) in bars.iter().enumerate() {
        sum += bar.close;
        if index >= window {
            sum -= bars[index - window].close;
        }
        if index + 1 >= window {
            out[index] = Some(sum / window as f64);
        }
    }
    out
}

fn date(time: i64) -> String {
    DateTime::<Utc>::from_timestamp(time, 0)
        .map(|stamp| stamp.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "—".to_string())
}

fn simulate(
    symbol: &str,
    bars: &[Bar],
    fast: usize,
    slow: usize,
    initial_cash: f64,
    fee_bps: f64,
) -> BacktestResult {
    let first_close = bars.first().map(|bar| bar.close).unwrap_or(0.0);
    let last_close = bars.last().map(|bar| bar.close).unwrap_or(0.0);
    let fee_rate = (fee_bps / 10_000.0).clamp(0.0, 0.01);

    let fast_sma = sma_series(bars, fast);
    let slow_sma = sma_series(bars, slow);

    let mut cash = initial_cash;
    let mut shares = 0.0;
    let mut entry: Option<Entry> = None;
    let mut pending: Option<Action> = None;
    let mut closed: Vec<ClosedTrade> = Vec::new();
    let mut curve: Vec<EquityPoint> = Vec::with_capacity(bars.len());
    let mut peak = initial_cash;
    let mut max_drawdown: f64 = 0.0;

    for (index, bar) in bars.iter().enumerate() {
        if let Some(action) = pending.take() {
            match action {
                Action::Buy if shares == 0.0 && bar.open > 0.0 && cash > 0.0 => {
                    let qty = cash * (1.0 - fee_rate) / bar.open;
                    if qty > 0.0 {
                        entry = Some(Entry {
                            time: bar.time,
                            price: bar.open,
                            qty,
                            cash_before: cash,
                        });
                        shares = qty;
                        cash = 0.0;
                    }
                }
                Action::Sell if shares > 0.0 && bar.open > 0.0 => {
                    let proceeds = shares * bar.open * (1.0 - fee_rate);
                    cash = proceeds;
                    if let Some(filled) = entry.take() {
                        let pnl = proceeds - filled.cash_before;
                        closed.push(ClosedTrade {
                            entry_time: filled.time,
                            exit_time: bar.time,
                            entry_price: filled.price,
                            exit_price: bar.open,
                            qty: filled.qty,
                            pnl,
                            return_pct: if filled.cash_before > 0.0 {
                                pnl / filled.cash_before * 100.0
                            } else {
                                0.0
                            },
                        });
                    }
                    shares = 0.0;
                }
                _ => {}
            }
        }

        let equity = cash + shares * bar.close;
        peak = peak.max(equity);
        if peak > 0.0 {
            max_drawdown = max_drawdown.min((equity - peak) / peak * 100.0);
        }

        let benchmark = if first_close > 0.0 {
            initial_cash * bar.close / first_close
        } else {
            initial_cash
        };
        curve.push(EquityPoint {
            time: bar.time,
            equity,
            benchmark,
        });

        if index + 1 >= bars.len() {
            continue;
        }

        if let (Some(fast_now), Some(fast_prev), Some(slow_now), Some(slow_prev)) = (
            fast_sma[index],
            index.checked_sub(1).and_then(|at| fast_sma[at]),
            slow_sma[index],
            index.checked_sub(1).and_then(|at| slow_sma[at]),
        ) {
            if fast_prev <= slow_prev && fast_now > slow_now && shares == 0.0 {
                pending = Some(Action::Buy);
            } else if fast_prev >= slow_prev && fast_now < slow_now && shares > 0.0 {
                pending = Some(Action::Sell);
            }
        }
    }

    let final_equity = cash + shares * last_close;
    let benchmark_final = if first_close > 0.0 {
        initial_cash * last_close / first_close
    } else {
        initial_cash
    };
    let wins = closed.iter().filter(|trade| trade.pnl > 0.0).count();

    let open_position = entry.map(|filled| OpenPosition {
        entry_time: filled.time,
        entry_price: filled.price,
        qty: shares,
        current_price: last_close,
        unrealized_pnl: shares * last_close - filled.cash_before,
    });

    BacktestResult {
        symbol: symbol.to_string(),
        strategy: STRATEGY.to_string(),
        fast,
        slow,
        days: 0,
        start: bars.first().map(|bar| date(bar.time)).unwrap_or_default(),
        end: bars.last().map(|bar| date(bar.time)).unwrap_or_default(),
        initial_cash,
        final_equity,
        total_return_pct: (final_equity / initial_cash - 1.0) * 100.0,
        benchmark_return_pct: (benchmark_final / initial_cash - 1.0) * 100.0,
        max_drawdown_pct: max_drawdown,
        win_rate_pct: if closed.is_empty() {
            0.0
        } else {
            wins as f64 / closed.len() as f64 * 100.0
        },
        trades: closed.len(),
        wins,
        equity_curve: curve,
        closed_trades: closed,
        open_position,
    }
}

#[tauri::command]
pub async fn run_backtest(
    state: State<'_, Arc<AppState>>,
    req: BacktestReq,
) -> Ipc<BacktestResult> {
    let params = req.validate()?;
    let session = state.session()?;
    let bars = daily_bars(&session, &params.symbol, params.days).await?;

    if bars.is_empty() {
        return Err(bad_request(format!("no daily bars for {}", params.symbol)).into());
    }
    if bars.len() < params.slow + 2 {
        return Err(bad_request("not enough bars for those SMA windows").into());
    }

    let mut result = simulate(
        &params.symbol,
        &bars,
        params.fast,
        params.slow,
        params.initial_cash,
        params.fee_bps,
    );
    result.days = params.days;

    if let Some(storage) = state.storage() {
        let params_json = serde_json::json!({
            "days": params.days,
            "fast": params.fast,
            "slow": params.slow,
            "initialCash": params.initial_cash,
            "feeBps": params.fee_bps,
        })
        .to_string();
        if let Ok(result_json) = serde_json::to_string(&result)
            && let Err(err) = storage.record_backtest(
                &params.symbol,
                STRATEGY,
                &params_json,
                &result_json,
            )
        {
            tracing::warn!(%err, "backtest not stored locally");
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(opens: &[f64], closes: &[f64]) -> Vec<Bar> {
        opens
            .iter()
            .zip(closes)
            .enumerate()
            .map(|(index, (open, close))| Bar {
                time: 1_700_000_000 + index as i64 * 86_400,
                open: *open,
                high: open.max(*close) + 1.0,
                low: open.min(*close) - 1.0,
                close: *close,
                volume: 1_000.0,
            })
            .collect()
    }

    #[test]
    fn signal_executes_on_the_next_open() {
        let closes = [10.0, 10.0, 10.0, 10.0, 20.0, 20.0, 20.0];
        let opens = [10.0, 10.0, 10.0, 10.0, 10.0, 200.0, 20.0];
        let result = simulate("AAPL", &bars(&opens, &closes), 2, 3, 1_000.0, 0.0);

        let open = result.open_position.expect("cross above should enter");
        assert_eq!(open.entry_price, 200.0);
        assert_eq!(result.trades, 0);
        assert_eq!(result.equity_curve.len(), closes.len());
    }

    #[test]
    fn flat_market_does_not_trade() {
        let closes = [100.0; 20];
        let result = simulate("AAPL", &bars(&closes, &closes), 2, 3, 10_000.0, 0.0);

        assert_eq!(result.trades, 0);
        assert!(result.open_position.is_none());
        assert!((result.total_return_pct).abs() < f64::EPSILON);
        assert!(result.max_drawdown_pct <= 0.0);
    }

    #[test]
    fn a_round_trip_books_win_and_drawdown() {
        let closes = [10.0, 10.0, 10.0, 10.0, 20.0, 30.0, 50.0, 60.0, 55.0, 40.0, 40.0];
        let result = simulate("AAPL", &bars(&closes, &closes), 2, 3, 1_000.0, 0.0);

        assert_eq!(result.trades, 1);
        assert!(result.closed_trades[0].pnl > 0.0);
        assert!(result.max_drawdown_pct < 0.0);
        assert!(result.win_rate_pct > 0.0);
    }

    #[test]
    fn fees_reduce_the_round_trip() {
        let closes = [10.0, 10.0, 10.0, 10.0, 20.0, 30.0, 50.0, 60.0, 55.0, 40.0, 40.0];
        let free = simulate("AAPL", &bars(&closes, &closes), 2, 3, 1_000.0, 0.0);
        let paid = simulate("AAPL", &bars(&closes, &closes), 2, 3, 1_000.0, 50.0);

        assert!(paid.final_equity < free.final_equity);
        assert_eq!(paid.trades, free.trades);
    }

    #[test]
    fn request_accepts_the_payload_the_ui_sends() {
        let req: BacktestReq = serde_json::from_str(
            r#"{"symbol":"AAPL","days":365,"fast":20,"slow":50,
                "initialCash":10000,"feeBps":0}"#,
        )
        .expect("backtest payload from ui/src/api.ts must deserialize");

        assert_eq!(req.symbol, "AAPL");
        assert_eq!(req.days, Some(365));
        assert_eq!(req.fast, Some(20));
        assert_eq!(req.slow, Some(50));
        assert_eq!(req.initial_cash, Some(10_000.0));
        assert_eq!(req.fee_bps, Some(0.0));
    }

    #[test]
    fn request_validation_rejects_bad_windows_and_cash() {
        let req = BacktestReq {
            symbol: "AAPL".to_string(),
            days: Some(365),
            fast: Some(50),
            slow: Some(20),
            initial_cash: Some(10_000.0),
            fee_bps: Some(0.0),
        };
        assert!(req.validate().is_err());

        let req = BacktestReq {
            symbol: "AAPL".to_string(),
            days: Some(365),
            fast: Some(20),
            slow: Some(50),
            initial_cash: Some(0.0),
            fee_bps: Some(0.0),
        };
        assert!(req.validate().is_err());
    }
}
