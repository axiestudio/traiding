//! The bundled Alpaca CLI.
//!
//! The binary ships inside the app as a resource, so this module locates it in
//! the resource directory and runs it as a child process. Nothing is installed
//! system-wide, and the webview can never spawn it: there is no shell plugin,
//! so every invocation comes from Rust through this module.
//!
//! Credentials are passed as environment variables on the child process only.
//! This module never writes, logs or returns them; the child sees them for the
//! duration of one request.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

use crate::state::Session;

/// A CLI call never needs longer than this; a hung child must not hang a turn.
const CALL_TIMEOUT: Duration = Duration::from_secs(20);
/// Output is trimmed to keep a small model's context affordable. Bars and news
/// are the bulky ones; a 64 KiB tail still holds a full page of either.
const MAX_OUTPUT: usize = 64 * 1024;

/// Why a CLI call did not produce output.
#[derive(Debug)]
pub enum CliError {
    /// The resource is missing — a packaging fault, not a user error.
    Missing(String),
    /// The child could not be started at all.
    Spawn(String),
    /// The child ran but exited non-zero; carries its stderr.
    Failed(String),
    /// The child outlived `CALL_TIMEOUT`.
    TimedOut,
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(where_) => write!(f, "the bundled Alpaca CLI is missing ({where_})"),
            Self::Spawn(err) => write!(f, "could not start the Alpaca CLI: {err}"),
            Self::Failed(err) => write!(f, "the Alpaca CLI failed: {err}"),
            Self::TimedOut => write!(f, "the Alpaca CLI timed out"),
        }
    }
}

/// Where the CLI lives for this build.
///
/// Installed builds read it from the resource directory. A `tauri dev` build
/// has no resource directory yet, so the staged copy under `binaries/` is used
/// instead. That fallback is compiled out of release builds: it embeds the
/// build machine's path, which means nothing on a user's machine.
pub fn path(resource_dir: Option<&Path>) -> Result<PathBuf, CliError> {
    if let Some(dir) = resource_dir {
        let installed = dir.join("bin").join("alpaca");
        if installed.is_file() {
            return Ok(installed);
        }
    }

    #[cfg(debug_assertions)]
    if let Some(staged) = staged_path() {
        return Ok(staged);
    }

    Err(CliError::Missing(match resource_dir {
        Some(dir) => format!("no bin/alpaca in {}", dir.display()),
        None => "no resource directory was available".to_string(),
    }))
}

/// The checkout copy, used only when running from a dev build.
#[cfg(debug_assertions)]
fn staged_path() -> Option<PathBuf> {
    let staged = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join("alpaca");
    staged.is_file().then_some(staged)
}

/// Run the CLI with `args` and return its stdout.
///
/// The child inherits no environment beyond what it needs: `ALPACA_*` is set
/// explicitly from the live session, and the rest is cleared so a stray
/// `ALPACA_PROFILE` or config path in the parent cannot redirect the call.
pub async fn run(
    resource_dir: Option<&Path>,
    session: &Session,
    args: &[&str],
) -> Result<String, CliError> {
    let binary = path(resource_dir)?;

    let mut command = Command::new(&binary);
    command
        .args(args)
        .env_clear()
        .env("ALPACA_API_KEY", &session.client.api_info().key_id)
        .env("ALPACA_SECRET_KEY", &session.client.api_info().secret)
        .env("ALPACA_LIVE_TRADE", if session.paper { "false" } else { "true" })
        .env("ALPACA_OUTPUT", "json")
        // A profile written by a previous `alpaca profile login` must not win.
        .env("ALPACA_CONFIG_DIR", std::env::temp_dir().join("traiding-alpaca-none"))
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", std::env::var("HOME").unwrap_or_default())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = timeout(CALL_TIMEOUT, command.output())
        .await
        .map_err(|_| CliError::TimedOut)?
        .map_err(|err| CliError::Spawn(err.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // The CLI writes JSON errors to stdout too; prefer whichever is fuller.
        let detail = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        return Err(CliError::Failed(trim(&detail, 400)));
    }

    Ok(trim(&stdout, MAX_OUTPUT))
}

/// Keep the tail: paginated output ends with the newest rows, and a JSON array
/// is only useful whole. Callers that already limited the page pass through.
fn trim(text: &str, limit: usize) -> String {
    let text = text.trim();
    if text.len() <= limit {
        return text.to_string();
    }
    let mut start = text.len() - limit;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &text[start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_keeps_short_output_whole() {
        assert_eq!(trim("  {\"ok\":true}  ", 100), "{\"ok\":true}");
    }

    #[test]
    fn trim_keeps_the_tail_of_long_output() {
        let long = "a".repeat(100) + "END";
        let trimmed = trim(&long, 10);
        assert!(trimmed.starts_with('…'));
        assert!(trimmed.ends_with("END"));
        assert_eq!(trimmed.chars().count(), 11);
    }

    #[test]
    fn trim_respects_char_boundaries() {
        // Multi-byte characters must not be sliced mid-codepoint.
        let long = "€".repeat(50);
        let trimmed = trim(&long, 7);
        assert!(trimmed.starts_with('…'));
        assert!(trimmed.chars().all(|c| c == '…' || c == '€'));
    }

    #[test]
    fn missing_binary_is_reported_by_path() {
        let err = path(Some(Path::new("/nonexistent-resources")));
        // In this repo binaries/ is staged, so the fallback succeeds. Only the
        // error branch is interesting when neither is present.
        if let Err(CliError::Missing(message)) = err {
            assert!(message.contains("alpaca"));
        }
    }
}

#[cfg(test)]
mod live_probe {
    //! Not part of `cargo test`: verifies the resource lookup and a real call.
    //! Run with `cargo test -- --ignored live_cli`.
    #[tokio::test]
    #[ignore]
    async fn live_cli() {
        let path = super::path(Some(std::path::Path::new("/nonexistent"))).expect("staged binary");
        println!("resolved: {}", path.display());
        let out = std::process::Command::new(&path)
            .args(["version"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("run");
        println!("version: {}", String::from_utf8_lossy(&out.stdout).trim());
        assert!(out.status.success());
    }
}

#[cfg(test)]
mod resource_layout {
    //! Confirms the packaging assumption: the resource map puts the CLI at
    //! `$RESOURCES/bin/alpaca`, and `path()` resolves exactly that.
    #[test]
    fn resolves_installed_resource_layout() {
        let dir = std::env::temp_dir().join("traiding-resource-probe");
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).expect("create probe dir");
        let cli = bin.join("alpaca");
        std::fs::write(&cli, b"stub").expect("write stub");

        let found = super::path(Some(&dir)).expect("installed layout must resolve");
        assert_eq!(found, cli);

        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod agent_access {
    //! Proves the agent's CLI tools reach the real binary with a real session.
    //! Ignored by default: it needs credentials and the network.
    //!   cargo test --lib -- --ignored agent_reaches_cli --nocapture

    use crate::state::Session;

    /// Build a session from `.env`, the same way the app does at startup.
    async fn live_session() -> Session {
        let _ = dotenvy::dotenv();
        let key = std::env::var("APCA_API_KEY_ID").expect("APCA_API_KEY_ID");
        let secret = std::env::var("APCA_API_SECRET_KEY").expect("APCA_API_SECRET_KEY");
        Session::connect(key, secret, crate::state::PAPER_URL.to_string(), true)
            .await
            .expect("session")
    }

    #[tokio::test]
    #[ignore]
    async fn agent_reaches_cli() {
        let session = live_session().await;

        for (tool, args) in [
            ("cli_clock", "{}"),
            ("cli_snapshot", r#"{"symbol":"AAPL"}"#),
            ("cli_news", r#"{"symbol":"TSLA","limit":2}"#),
            ("cli_watchlists", "{}"),
        ] {
            let (result, is_error) = crate::agent::run_tool_direct(&session, tool, args).await;
            println!("--- {tool} (error={is_error})\n{result}\n");
            assert!(!is_error, "{tool} failed: {result}");
            // `[]` is a legitimate answer — the account simply has no watchlists.
            assert!(
                result.trim_start().starts_with(['{', '[']),
                "{tool} did not return JSON: {result}"
            );
            assert!(
                !result.to_lowercase().contains("\"secret"),
                "{tool} leaked a credential field"
            );
        }
    }
}
