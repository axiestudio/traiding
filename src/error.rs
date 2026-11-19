//! Error type shared by all IPC commands.
//!
//! `apca` reports upstream failures through a generated, per-endpoint error
//! enum whose `Display` output looks like
//! `HTTP status 422: {"code":40010001,"message":"insufficient buying power"}`.
//! That is not something we want to show in a toast, so [`from_request_error`]
//! rewrites it into a short, human readable sentence.
//!
//! Commands return `Result<T, String>`: Tauri serialises the error straight to
//! the webview, and the UI treats it as the message to display.

use apca::RequestError;

#[derive(Debug, Clone)]
pub enum AppError {
    /// No credentials are configured, or they were rejected upstream.
    Unauthorized(String),
    /// The request was well formed but not acceptable.
    BadRequest(String),
    /// The upstream Alpaca API returned an error.
    Upstream(String),
    Internal(String),
}

impl AppError {
    fn message(&self) -> &str {
        match self {
            Self::Unauthorized(m) | Self::BadRequest(m) | Self::Upstream(m) | Self::Internal(m) => m,
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for AppError {}

/// Errors cross the IPC boundary as their display string.
impl From<AppError> for String {
    fn from(err: AppError) -> Self {
        err.to_string()
    }
}

impl From<apca::Error> for AppError {
    fn from(err: apca::Error) -> Self {
        Self::Internal(err.to_string())
    }
}

/// Translate an SDK request failure into an [`AppError`] with a clean message.
pub fn from_request_error<E: std::fmt::Display>(err: RequestError<E>) -> AppError {
    match err {
        RequestError::Endpoint(e) => {
            let raw = e.to_string();
            let (status, message) = split_endpoint_error(&raw);
            match status {
                Some(401) | Some(403) => {
                    AppError::Unauthorized(format!("Alpaca rejected the credentials: {message}"))
                }
                _ => AppError::Upstream(message),
            }
        }
        RequestError::Hyper(e) => AppError::Upstream(format!("network error: {e}")),
        RequestError::HyperUtil(e) => AppError::Upstream(format!("network error: {e}")),
        RequestError::Io(e) => AppError::Upstream(format!("transport error: {e}")),
    }
}

/// Split `HTTP status 422 Unprocessable Entity: {"message":"..."}` into
/// `(Some(422), "...")`.
///
/// `http::StatusCode`'s `Display` renders the reason phrase alongside the code
/// ("422 Unprocessable Entity"), so only the leading digits are the code.
fn split_endpoint_error(raw: &str) -> (Option<u16>, String) {
    let mut rest = raw;

    if let Some(tail) = rest.strip_prefix("HTTP status ") {
        rest = tail;
    } else if let Some(tail) = rest.strip_prefix("Unexpected HTTP status ") {
        rest = tail;
    } else {
        return (None, raw.to_string());
    }

    let mut status = None;
    if let Some((code, tail)) = rest.split_once(':') {
        let digits: String = code
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        status = digits.parse::<u16>().ok();
        rest = tail.trim();
    }

    // Alpaca always answers errors with `{"code":..., "message":"..."}`, but a
    // bad API key comes back as the bare string `unauthorized.`
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(rest)
        && let Some(message) = value.get("message").and_then(|m| m.as_str())
    {
        let code = value.get("code").and_then(|c| c.as_u64());
        return (
            status,
            match code {
                Some(code) => format!("{message} (code {code})"),
                None => message.to_string(),
            },
        );
    }

    let trimmed = rest.trim_matches('"').trim();
    (
        status,
        if trimmed.is_empty() {
            raw.to_string()
        } else {
            trimmed.to_string()
        },
    )
}

/// Convenience helpers used by argument validation in `commands`.
pub fn bad_request(message: impl Into<String>) -> AppError {
    AppError::BadRequest(message.into())
}
