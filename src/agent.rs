//! BYOK chat agent.
//!
//! The agent talks to OpenRouter from inside the Rust process. The API key is
//! read from the environment, never stored in SQLite and never sent to the
//! webview — the UI only ever sees a boolean saying whether a key is present.
//!
//! Tools reuse the same `apca` client as the rest of the terminal, so what the
//! agent reads is exactly what the panes show.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::commands::{daily_bars, normalize_symbol};
use crate::error::AppError;
use crate::state::{AppState, Session};

type Ipc<T> = Result<T, String>;

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const DEFAULT_MODEL: &str = "inclusionai/ling-3.2-flash:free";
/// Free models allow tool loops, but a runaway loop must still terminate.
const MAX_TOOL_ROUNDS: usize = 4;
/// A stalled provider connection must not leave the composer busy forever.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    /// Always `function` today; kept so the round-trip is lossless.
    #[serde(rename = "type", default = "function_type")]
    pub kind: String,
    pub function: FunctionCall,
}

fn function_type() -> String {
    "function".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    /// JSON encoded as a string, exactly as the provider sends it.
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

/// One chunk pushed to the webview over the IPC channel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "type")]
pub enum StreamEvent {
    /// A slice of assistant text.
    Delta { text: String },
    /// The model asked for a tool; the UI shows it while Rust runs it.
    ToolStart { id: String, name: String, arguments: String },
    /// The tool finished. `result` is already JSON.
    ToolDone { id: String, name: String, result: String, is_error: bool },
    /// Terminal event: the turn finished and the full text is known.
    Done { text: String, rounds: usize },
}

/// Request body from the UI.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentChatReq {
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub model: Option<String>,
}

/// What the UI is allowed to know about the key.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub configured: bool,
    pub model: String,
}

// ---------------------------------------------------------------------------
// Provider payloads
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct CompletionReq<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a serde_json::Value>,
    /// Ling 3.2 Flash is hybrid-reasoning. Left on, it streams `reasoning`
    /// deltas while `content` stays empty, so the pane looks frozen and the
    /// token budget goes to thinking before any answer appears.
    reasoning: ReasoningPref,
}

#[derive(Serialize)]
struct ReasoningPref {
    enabled: bool,
}

#[derive(Deserialize)]
struct CompletionResp {
    #[serde(default)]
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Option<AssistantMessage>,
    delta: Option<Delta>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct AssistantMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Deserialize)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(Deserialize)]
struct ApiError {
    error: ApiErrorBody,
}

#[derive(Deserialize)]
struct ApiErrorBody {
    message: String,
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

/// The tool surface the model sees. Descriptions are short on purpose: a small
/// model spends less of its context guessing what a tool does.
fn tool_specs() -> serde_json::Value {
    serde_json::json!([
        {
            "type": "function",
            "function": {
                "name": "get_quote",
                "description": "Latest bid/ask quote for one US equity symbol.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string", "description": "Ticker, e.g. AAPL" }
                    },
                    "required": ["symbol"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_bars",
                "description": "Recent daily candles for a symbol, oldest first.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string" },
                        "days": { "type": "integer", "description": "Lookback in days, 1-365" }
                    },
                    "required": ["symbol"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_positions",
                "description": "Open positions on the connected account.",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_account",
                "description": "Account equity, cash and buying power.",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_orders",
                "description": "Recent orders, newest first.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "enum": ["open", "closed", "all"],
                            "description": "Defaults to all"
                        }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "cli_snapshot",
                "description": "Full market snapshot for a symbol: latest trade, quote, and daily bar.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string", "description": "Ticker, e.g. AAPL" }
                    },
                    "required": ["symbol"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "cli_news",
                "description": "Recent news headlines, optionally for one symbol.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string", "description": "Ticker, e.g. TSLA" },
                        "limit": { "type": "integer", "description": "How many articles, 1-20" }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "cli_clock",
                "description": "Whether the market is open now, with the next open and close.",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "cli_watchlists",
                "description": "Watchlists on the connected account and their symbols.",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "cli_calendar",
                "description": "Market trading days between two dates.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "start": { "type": "string", "description": "YYYY-MM-DD" },
                        "end": { "type": "string", "description": "YYYY-MM-DD" }
                    }
                }
            }
        }
    ])
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn agent_status() -> Ipc<AgentStatus> {
    Ok(AgentStatus {
        configured: key().is_some(),
        model: std::env::var("AGENT_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
    })
}

/// Run one assistant turn, streaming events back over `on_event`.
///
/// Tool calls are executed here, not in the webview: the model never gets a
/// handle it could use to call arbitrary commands.
#[tauri::command]
pub async fn agent_chat(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    req: AgentChatReq,
    on_event: Channel<StreamEvent>,
) -> Ipc<()> {
    let key = key().ok_or_else(|| {
        "no OpenRouter key configured — set OPENROUTER_API_KEY in .env and restart".to_string()
    })?;
    let model = req.model.unwrap_or_else(|| {
        std::env::var("AGENT_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string())
    });

    let http = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|err| format!("could not start the agent HTTP client: {err}"))?;

    // The provider is stateless, so every turn starts with the same brief. The
    // UI never sees it and cannot edit it: the tool boundary and the read-only
    // rule stay under Rust's control.
    let mut messages = Vec::with_capacity(req.messages.len() + 1);
    messages.push(ChatMessage {
        role: "system".to_string(),
        content: Some(system_prompt(state.session().ok().map(|session| session.paper))),
        tool_calls: None,
        tool_call_id: None,
    });
    messages.extend(req.messages);
    let mut collected = String::new();

    for round in 0..MAX_TOOL_ROUNDS {
        let specs = tool_specs();
        let body = CompletionReq {
            model: &model,
            messages: &messages,
            stream: true,
            tools: Some(&specs),
            reasoning: ReasoningPref {
                enabled: thinking_enabled(),
            },
        };

        let request = http
            .post(OPENROUTER_URL)
            .bearer_auth(&key)
            .header("HTTP-Referer", "https://github.com/axiestudio/traiding")
            .header("X-Title", "traiding")
            .json(&body);

        let turn = stream_turn(request, &on_event).await?;

        // The text of this round is part of the visible answer.
        collected.push_str(&turn.text);

        let Some(calls) = turn.tool_calls.filter(|calls| !calls.is_empty()) else {
            let _ = on_event.send(StreamEvent::Done {
                text: collected.clone(),
                rounds: round + 1,
            });
            return Ok(());
        };

        // Echo the assistant turn back so the model keeps its own reasoning.
        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: if turn.text.is_empty() {
                None
            } else {
                Some(turn.text)
            },
            tool_calls: Some(calls.clone()),
            tool_call_id: None,
        });

        for call in calls {
            let arguments = call.function.arguments.clone();
            let _ = on_event.send(StreamEvent::ToolStart {
                id: call.id.clone(),
                name: call.function.name.clone(),
                arguments: arguments.clone(),
            });

            let (result, is_error) =
                run_tool(Some(&app), &state, &call.function.name, &arguments).await;

            let _ = on_event.send(StreamEvent::ToolDone {
                id: call.id.clone(),
                name: call.function.name.clone(),
                result: result.clone(),
                is_error,
            });

            messages.push(ChatMessage {
                role: "tool".to_string(),
                content: Some(result),
                tool_calls: None,
                tool_call_id: Some(call.id),
            });
        }
    }

    // Loop budget exhausted: return what we have rather than hanging.
    let _ = on_event.send(StreamEvent::Done {
        text: if collected.is_empty() {
            "Stopped after too many tool rounds.".to_string()
        } else {
            collected
        },
        rounds: MAX_TOOL_ROUNDS,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

struct Turn {
    text: String,
    tool_calls: Option<Vec<ToolCall>>,
}

/// Read an SSE completion to completion, emitting text deltas as they arrive
/// and accumulating tool calls by index.
async fn stream_turn(
    request: reqwest::RequestBuilder,
    on_event: &Channel<StreamEvent>,
) -> Result<Turn, String> {
    let response = request.send().await.map_err(net)?;
    let status = response.status();

    if !status.is_success() {
        let raw = response.text().await.unwrap_or_default();
        return Err(provider_error(status.as_u16(), &raw));
    }

    let mut buffer = String::new();
    let mut text = String::new();
    let mut calls: Vec<ToolCall> = Vec::new();
    let mut stream = response.bytes_stream();

    use futures::StreamExt;
    'outer: while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(net)?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // SSE frames are newline separated; keep the trailing partial line.
        while let Some(newline) = buffer.find('\n') {
            let line = buffer[..newline].trim_end_matches('\r').to_string();
            buffer.drain(..=newline);

            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data.is_empty() {
                continue;
            }
            if data == "[DONE]" {
                break 'outer;
            }

            let Ok(parsed) = serde_json::from_str::<CompletionResp>(data) else {
                continue; // keepalive or an unknown frame
            };
            let Some(choice) = parsed.choices.into_iter().next() else {
                continue;
            };

            if let Some(delta) = choice.delta {
                if let Some(piece) = delta.content
                    && !piece.is_empty()
                {
                    text.push_str(&piece);
                    let _ = on_event.send(StreamEvent::Delta { text: piece });
                }
                if let Some(deltas) = delta.tool_calls {
                    for part in deltas {
                        merge_tool_call(&mut calls, part);
                    }
                }
            }

            // Non-streaming providers answer in one shot; accept that shape too.
            if let Some(message) = choice.message {
                if let Some(piece) = message.content
                    && !piece.is_empty()
                {
                    text.push_str(&piece);
                    let _ = on_event.send(StreamEvent::Delta { text: piece.clone() });
                }
                if let Some(all) = message.tool_calls {
                    for call in all {
                        calls.push(call);
                    }
                }
            }

            let _ = choice.finish_reason;
        }
    }

    // Tool-call deltas arrive split across frames; drop anything that never got
    // a name, since it cannot be dispatched.
    calls.retain(|call| !call.function.name.is_empty());

    Ok(Turn {
        text,
        tool_calls: (!calls.is_empty()).then_some(calls),
    })
}

/// Providers stream a tool call as many deltas keyed by `index`: the id and name
/// arrive once, the argument string in pieces.
fn merge_tool_call(calls: &mut Vec<ToolCall>, part: ToolCallDelta) {
    while calls.len() <= part.index {
        calls.push(ToolCall {
            id: String::new(),
            kind: "function".to_string(),
            function: FunctionCall {
                name: String::new(),
                arguments: String::new(),
            },
        });
    }

    let slot = &mut calls[part.index];
    if let Some(id) = part.id
        && !id.is_empty()
    {
        slot.id = id;
    }
    if let Some(kind) = part.kind
        && !kind.is_empty()
    {
        slot.kind = kind;
    }
    if let Some(function) = part.function {
        if let Some(name) = function.name
            && !name.is_empty()
        {
            slot.function.name = name;
        }
        if let Some(arguments) = function.arguments {
            slot.function.arguments.push_str(&arguments);
        }
    }
}

// ---------------------------------------------------------------------------
// Tool execution
// ---------------------------------------------------------------------------

/// Run one tool call and return `(json, is_error)`.
async fn run_tool(
    app: Option<&tauri::AppHandle>,
    state: &State<'_, Arc<AppState>>,
    name: &str,
    raw_args: &str,
) -> (String, bool) {
    match dispatch(app, state, name, raw_args).await {
        Ok(value) => (value.to_string(), false),
        Err(err) => (serde_json::json!({ "error": err }).to_string(), true),
    }
}

/// Same dispatch, without a Tauri `State`.
///
/// `State` can only be built by the runtime, so this seam lets a test drive the
/// real tool code — including the CLI subprocess — against a live session.
#[cfg(test)]
pub(crate) async fn run_tool_direct(
    session: &Session,
    name: &str,
    raw_args: &str,
) -> (String, bool) {
    let args: serde_json::Value =
        serde_json::from_str(raw_args).unwrap_or_else(|_| serde_json::json!({}));

    let result = if name.starts_with("cli_") {
        dispatch_cli(None, session, name, &args).await
    } else {
        Err(format!("`{name}` is not a CLI tool"))
    };

    match result {
        Ok(value) => (value, false),
        Err(err) => (serde_json::json!({ "error": err }).to_string(), true),
    }
}

async fn dispatch(
    app: Option<&tauri::AppHandle>,
    state: &State<'_, Arc<AppState>>,
    name: &str,
    raw_args: &str,
) -> Result<serde_json::Value, String> {
    let args: serde_json::Value =
        serde_json::from_str(raw_args).unwrap_or_else(|_| serde_json::json!({}));
    let session = state.session().map_err(|err| err.to_string())?;

    // The CLI-backed tools return their JSON text verbatim: the model reads
    // the provider's own field names, and re-encoding would only lose detail.
    if name.starts_with("cli_") {
        let text = dispatch_cli(app, &session, name, &args).await?;
        return Ok(serde_json::Value::String(text));
    }

    match name {
        "get_quote" => {
            let symbol = arg_symbol(&args)?;
            let request = apca::data::v2::last_quotes::GetReqInit::default().init([symbol.clone()]);
            let quotes = session
                .client
                .issue::<apca::data::v2::last_quotes::Get>(&request)
                .await
                .map_err(|err| crate::error::from_request_error(err).to_string())?;

            let Some((symbol, quote)) = quotes.into_iter().next() else {
                return Err(format!("no quote returned for {symbol}"));
            };
            Ok(serde_json::json!({
                "symbol": symbol,
                "bid": quote.bid_price.to_string(),
                "ask": quote.ask_price.to_string(),
                "bidSize": quote.bid_size,
                "askSize": quote.ask_size,
                "time": quote.time.to_rfc3339()
            }))
        }

        "get_bars" => {
            let symbol = arg_symbol(&args)?;
            let days = args
                .get("days")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(30)
                .clamp(1, 365);
            let bars = daily_bars(&session, &symbol, days)
                .await
                .map_err(|err| err.to_string())?;

            // Only the tail: a small model does not need 365 rows of context.
            let tail: Vec<_> = bars.iter().rev().take(30).rev().collect();
            Ok(serde_json::json!({
                "symbol": symbol,
                "count": bars.len(),
                "recent": tail.iter().map(|bar| serde_json::json!({
                    "time": bar.time,
                    "open": bar.open,
                    "high": bar.high,
                    "low": bar.low,
                    "close": bar.close,
                    "volume": bar.volume
                })).collect::<Vec<_>>()
            }))
        }

        "get_positions" => {
            let positions = session
                .client
                .issue::<apca::api::v2::positions::List>(&())
                .await
                .map_err(|err| crate::error::from_request_error(err).to_string())?;
            serde_json::to_value(positions).map_err(|err| err.to_string())
        }

        "get_account" => {
            let account = session
                .client
                .issue::<apca::api::v2::account::Get>(&())
                .await
                .map_err(|err| crate::error::from_request_error(err).to_string())?;
            serde_json::to_value(account).map_err(|err| err.to_string())
        }

        "get_orders" => {
            let status = match args.get("status").and_then(serde_json::Value::as_str) {
                Some("open") => apca::api::v2::orders::Status::Open,
                Some("closed") => apca::api::v2::orders::Status::Closed,
                _ => apca::api::v2::orders::Status::All,
            };
            let request = apca::api::v2::orders::ListReq {
                status,
                limit: Some(25),
                ..Default::default()
            };
            let orders = session
                .client
                .issue::<apca::api::v2::orders::List>(&request)
                .await
                .map_err(|err| crate::error::from_request_error(err).to_string())?;
            serde_json::to_value(orders).map_err(|err| err.to_string())
        }

        other => Err(format!("unknown tool `{other}`")),
    }
}

/// CLI-backed tools.
///
/// These go through the bundled binary rather than the `apca` client because
/// the CLI already shapes its output for reading and covers endpoints the
/// client does not wrap. `path::resource_dir` is `None` outside a real app, in
/// which case `cli::run` falls back to the staged copy.
async fn dispatch_cli(
    app: Option<&tauri::AppHandle>,
    session: &Session,
    name: &str,
    args: &serde_json::Value,
) -> Result<String, String> {
    use tauri::Manager;

    let resource_dir = app.and_then(|handle| handle.path().resource_dir().ok());

    let owned: Vec<String> = match name {
        "cli_snapshot" => vec![
            "data".into(),
            "snapshot".into(),
            "--symbol".into(),
            arg_symbol(args)?,
        ],
        "cli_news" => {
            let limit = args
                .get("limit")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(10)
                .clamp(1, 20);
            let mut argv = vec!["data".to_string(), "news".to_string()];
            if args.get("symbol").and_then(serde_json::Value::as_str).is_some() {
                argv.push("--symbols".into());
                argv.push(arg_symbol(args)?);
            }
            argv.push("--limit".into());
            argv.push(limit.to_string());
            // Headlines only: article bodies would swamp the context window.
            argv.push("--exclude-contentless".into());
            argv
        }
        "cli_clock" => vec!["clock".into()],
        "cli_watchlists" => vec!["watchlist".into(), "list".into()],
        "cli_calendar" => {
            let mut argv = vec!["calendar".to_string()];
            for (flag, key) in [("--start", "start"), ("--end", "end")] {
                if let Some(value) = arg_date(args, key)? {
                    argv.push(flag.into());
                    argv.push(value);
                }
            }
            argv
        }
        other => return Err(format!("unknown CLI tool `{other}`")),
    };

    let argv: Vec<&str> = owned.iter().map(String::as_str).collect();
    crate::cli::run(resource_dir.as_deref(), session, &argv)
        .await
        .map_err(|err| err.to_string())
}

/// Calendar arguments are passed to the CLI as separate argv entries, but a
/// value beginning with `--` would still be parsed as a flag. Accept only the
/// documented `YYYY-MM-DD` shape.
fn arg_date(args: &serde_json::Value, key: &str) -> Result<Option<String>, String> {
    let Some(raw) = args.get(key).and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    let value = raw.trim();
    if value.is_empty() {
        return Ok(None);
    }
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("`{value}` is not a YYYY-MM-DD date"))?;
    Ok(Some(value.to_string()))
}

fn arg_symbol(args: &serde_json::Value) -> Result<String, String> {
    args.get("symbol")
        .and_then(serde_json::Value::as_str)
        .map(normalize_symbol)
        .transpose()
        .map_err(|err| AppError::to_string(&err))?
        .filter(|symbol| !symbol.is_empty())
        .ok_or_else(|| "a symbol is required".to_string())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The brief every turn starts with.
///
/// The model is stateless between turns, so this is rebuilt each time. It
/// keeps the assistant inside its read-only tool surface and tells it which
/// account the numbers belong to.
fn system_prompt(paper: Option<bool>) -> String {
    let account = match paper {
        Some(true) => "a paper-trading account",
        Some(false) => "a live-money account",
        None => "an account that is not connected",
    };
    format!(
        "You are the assistant inside traiding, a desktop Alpaca trading terminal. \
         You are querying {account}. Use the provided tools for quotes, bars, positions, \
         account, orders, news, watchlists and the market calendar; never guess a price, \
         position or order status. You can read data only: you cannot place, change or \
         cancel orders, so send the user to the Trade tab for that. Today is {}. \
         Keep replies under 100 words and use plain text or a short list. \
         Do not give financial advice.",
        chrono::Utc::now().format("%Y-%m-%d")
    )
}

/// Read the key without ever logging or returning it.
fn key() -> Option<String> {
    std::env::var("OPENROUTER_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Reasoning is off unless `AGENT_REASONING=1`.
///
/// Hybrid-reasoning models stream `reasoning` deltas with an empty `content`
/// field. The pane renders content only, so leaving it on makes every answer
/// look stalled until thinking finishes. Off by default; opt in to compare.
fn thinking_enabled() -> bool {
    std::env::var("AGENT_REASONING")
        .map(|value| matches!(value.trim(), "1" | "true" | "on"))
        .unwrap_or(false)
}

fn net(err: reqwest::Error) -> String {
    format!("network error talking to OpenRouter: {err}")
}

/// Turn an OpenRouter error body into the sentence shown in the chat.
fn provider_error(status: u16, raw: &str) -> String {
    let detail = serde_json::from_str::<ApiError>(raw)
        .map(|body| body.error.message)
        .unwrap_or_else(|_| raw.trim().to_string());

    match status {
        401 => "OpenRouter rejected the API key.".to_string(),
        402 => "OpenRouter credits are exhausted.".to_string(),
        403 => "OpenRouter blocked this key or spend limit.".to_string(),
        404 => format!("model unavailable: {detail}"),
        429 => "OpenRouter rate limit hit — retry in a moment.".to_string(),
        502 => "the model provider failed upstream; that request was not billed.".to_string(),
        _ if detail.is_empty() => format!("OpenRouter returned HTTP {status}"),
        _ => detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The UI sends camelCase; a rename here breaks the composer silently.
    #[test]
    fn chat_message_round_trips_camel_case() {
        let raw = r#"{"role":"assistant","content":null,
            "tool_calls":[{"id":"c1","type":"function",
            "function":{"name":"get_quote","arguments":"{\"symbol\":\"AAPL\"}"}}]}"#;

        let message: ChatMessage = serde_json::from_str(raw).unwrap();
        assert_eq!(message.role, "assistant");
        let call = &message.tool_calls.unwrap()[0];
        assert_eq!(call.function.name, "get_quote");
        assert_eq!(call.function.arguments, r#"{"symbol":"AAPL"}"#);
    }

    /// Tool arguments arrive split across frames; merging must join them in
    /// order and not clobber the id or name on later fragments.
    #[test]
    fn tool_call_deltas_merge_by_index() {
        let mut calls: Vec<ToolCall> = Vec::new();

        merge_tool_call(&mut calls, ToolCallDelta {
            index: 0,
            id: Some("call_1".to_string()),
            kind: Some("function".to_string()),
            function: Some(FunctionDelta {
                name: Some("get_bars".to_string()),
                arguments: Some("{\"sym".to_string()),
            }),
        });
        merge_tool_call(&mut calls, ToolCallDelta {
            index: 0,
            id: None,
            kind: None,
            function: Some(FunctionDelta {
                name: None,
                arguments: Some("bol\":\"AAPL\"}".to_string()),
            }),
        });

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "get_bars");
        assert_eq!(calls[0].function.arguments, r#"{"symbol":"AAPL"}"#);
    }

    /// A frame that never carried a name cannot be dispatched.
    #[test]
    fn nameless_tool_calls_are_dropped() {
        let mut calls: Vec<ToolCall> = Vec::new();
        merge_tool_call(&mut calls, ToolCallDelta {
            index: 0,
            id: Some("call_x".to_string()),
            kind: None,
            function: Some(FunctionDelta {
                name: None,
                arguments: Some("{}".to_string()),
            }),
        });
        calls.retain(|call| !call.function.name.is_empty());
        assert!(calls.is_empty());
    }

    #[test]
    fn provider_errors_are_short_sentences() {
        assert_eq!(
            provider_error(401, r#"{"error":{"message":"bad key"}}"#),
            "OpenRouter rejected the API key."
        );
        assert_eq!(
            provider_error(402, "{}"),
            "OpenRouter credits are exhausted."
        );
        assert_eq!(
            provider_error(400, r#"{"error":{"message":"unsupported parameter"}}"#),
            "unsupported parameter"
        );
    }

    /// The browser matches these field names. A rename shows an empty tool card
    /// or leaves the pane stuck on `running`.
    #[test]
    fn stream_events_serialize_camel_case() {
        let delta = serde_json::to_value(StreamEvent::Delta { text: "hi".into() }).unwrap();
        assert_eq!(delta["type"], "delta");
        assert_eq!(delta["text"], "hi");

        let done = serde_json::to_value(StreamEvent::Done {
            text: "answer".into(),
            rounds: 2,
        })
        .unwrap();
        assert_eq!(done["type"], "done");
        assert_eq!(done["rounds"], 2);

        let tool = serde_json::to_value(StreamEvent::ToolDone {
            id: "call_1".into(),
            name: "get_quote".into(),
            result: "{}".into(),
            is_error: true,
        })
        .unwrap();
        assert_eq!(tool["type"], "toolDone");
        assert_eq!(tool["id"], "call_1");
        assert_eq!(tool["isError"], true);
    }

    /// The prompt is the only place the model learns it cannot trade.
    #[test]
    fn system_prompt_names_the_account_and_read_only_rule() {
        let paper = system_prompt(Some(true));
        assert!(paper.contains("paper-trading account"));
        assert!(paper.contains("cannot place"));

        assert!(system_prompt(Some(false)).contains("live-money account"));
        assert!(system_prompt(None).contains("not connected"));
    }

    /// `--start` values go straight to the CLI's argv, so only real dates pass.
    #[test]
    fn calendar_dates_are_strict() {
        let args = serde_json::json!({ "start": "2026-01-01", "end": "not-a-date" });
        assert_eq!(
            arg_date(&args, "start").unwrap().as_deref(),
            Some("2026-01-01")
        );
        assert!(arg_date(&args, "end").is_err());
        assert!(arg_date(&serde_json::json!({}), "start").unwrap().is_none());
    }

    #[test]
    fn tool_specs_are_well_formed() {
        let specs = tool_specs();
        let list = specs.as_array().expect("tools must be an array");
        assert!(list.len() >= 4);

        for tool in list {
            assert_eq!(tool["type"], "function");
            let function = &tool["function"];
            assert!(function["name"].as_str().is_some_and(|n| !n.is_empty()));
            assert!(function["description"].as_str().is_some_and(|d| !d.is_empty()));
            assert_eq!(function["parameters"]["type"], "object");
        }
    }
}
