//! `traiding_lib` — the desktop shell around the Alpaca terminal.
//!
//! Startup wires three things: tracing, an optional pre-authenticated session
//! from the environment, and the Tauri window whose webview hosts the UI.

mod agent;
mod backtest;
mod cli;
mod commands;
mod config;
mod error;
mod state;
mod storage;
mod stream;

use std::sync::Arc;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;
use crate::storage::Storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("traiding=info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second process would fight the first for the one Alpaca
            // market-data socket, so raise the window that already has it.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let state = Arc::new(AppState::new());
            open_storage(app.handle(), &state);
            log_cli_location(app.handle());
            load_config_env(app.handle());
            preauthenticate(&state);
            if state.session().is_ok() {
                state.start_order_stream(app.handle().clone());
            }
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_session,
            commands::connect,
            commands::disconnect,
            commands::get_account,
            commands::get_clock,
            commands::list_positions,
            commands::close_position,
            commands::list_orders,
            commands::create_order,
            commands::cancel_order,
            commands::list_assets,
            commands::get_asset,
            commands::get_bars,
            commands::get_quote,
            commands::set_stream_symbol,
            commands::feed_status,
            backtest::run_backtest,
            commands::get_overview,
            commands::get_ui_state,
            commands::save_ui_state,
            commands::get_storage_info,
            agent::agent_status,
            agent::agent_set_key,
            agent::agent_chat,
        ])
        .run(tauri::generate_context!())
        .expect("error while running traiding");
}

/// Open the local database under Tauri's app-local data directory.
///
/// Linux: `~/.local/share/<identifier>/storage/traiding.db`
/// macOS: `~/Library/Application Support/<identifier>/storage/traiding.db`
/// Windows: `%LOCALAPPDATA%\<identifier>\storage\traiding.db`
///
/// A missing database is not fatal: the terminal still works, it just stops
/// remembering preferences and history for that run.
fn open_storage(app: &tauri::AppHandle, state: &Arc<AppState>) {
    let directory = match app.path().app_local_data_dir() {
        Ok(directory) => directory,
        Err(err) => {
            tracing::warn!(%err, "could not resolve the app-local data directory");
            return;
        }
    };
    let path = directory.join("storage").join("traiding.db");
    match Storage::open(path) {
        Ok(storage) => {
            tracing::info!(path = %storage.path().display(), "local storage ready");
            state.set_storage(storage);
        }
        Err(err) => {
            tracing::warn!(%err, "local storage unavailable; running without persistence");
        }
    }
}

/// Log where the bundled CLI was found, or why it was not.
///
/// Resolution depends on packaging, so a missing binary is a build fault worth
/// seeing at startup rather than only when a tool call fails.
fn log_cli_location(app: &tauri::AppHandle) {
    let resource_dir = app.path().resource_dir().ok();
    match cli::path(resource_dir.as_deref()) {
        Ok(path) => tracing::info!(path = %path.display(), "bundled Alpaca CLI ready"),
        Err(err) => {
            tracing::warn!(%err, "bundled Alpaca CLI unavailable — agent CLI tools will fail")
        }
    }
}

/// Load `.env` from the app config directory.
///
/// The AppImage runs with its mount as the working directory, so a project
/// `.env` is never found. This gives installed builds a stable place for the
/// Alpaca pair and OpenRouter key that Rust writes at mode 0600.
fn load_config_env(app: &tauri::AppHandle) {
    if let Ok(dir) = app.path().app_config_dir() {
        let _ = dotenvy::from_path(dir.join(".env"));
    }
}

/// Optional pre-authentication from the environment so the window is live on
/// first paint. A failed attempt is not fatal: the UI falls back to its connect
/// dialog.
///
/// The endpoint is derived from the key's prefix rather than assumed, because a
/// key is only valid on its own host: a live key sent to the paper endpoint is
/// rejected with a bare `unauthorized.` that looks like a typo in the secret.
fn preauthenticate(state: &Arc<AppState>) {
    // Prefer the paper pair when present. Developing against paper is the norm,
    // and `APCA_API_KEY_ID` may hold live keys used for real trading.
    let credentials = std::env::var("APCA_TEST_API_KEY_ID")
        .ok()
        .zip(std::env::var("APCA_TEST_API_SECRET_KEY").ok())
        .filter(|(key, secret)| !key.trim().is_empty() && !secret.trim().is_empty())
        .or_else(|| {
            std::env::var("APCA_API_KEY_ID")
                .ok()
                .zip(std::env::var("APCA_API_SECRET_KEY").ok())
                .filter(|(key, secret)| !key.trim().is_empty() && !secret.trim().is_empty())
        });

    let Some((key_id, secret)) = credentials else {
        return;
    };

    let explicit_base = std::env::var("APCA_API_BASE_URL").ok();
    let inferred = state::infer_paper(&key_id);

    if let (Some(url), Some(key_paper)) = (&explicit_base, inferred) {
        let url_paper = !url.contains("//api.alpaca.markets");
        if url_paper != key_paper {
            // Rather than fire a request that cannot succeed, skip the startup
            // login and let the user resolve it in the UI, where the mode
            // toggle is visible.
            tracing::warn!(
                base_url = %url,
                "APCA_API_BASE_URL points at the {} host but the configured key is a {} key; \
                 skipping startup login, pick the matching mode in the UI",
                if url_paper { "paper" } else { "live" },
                if key_paper { "paper" } else { "live" },
            );
            return;
        }
    }

    let (base_url, paper) = match explicit_base {
        Some(url) => {
            let paper = !url.contains("//api.alpaca.markets");
            (url, paper)
        }
        None => match inferred {
            Some(paper) if paper => (state::PAPER_URL.to_string(), true),
            Some(_) => (state::LIVE_URL.to_string(), false),
            // Unrecognised prefix: keep the historical default.
            None => (state::PAPER_URL.to_string(), true),
        },
    };

    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(err) => {
            tracing::warn!(%err, "could not start a runtime for the startup login");
            return;
        }
    };

    match rt.block_on(state::Session::connect(key_id, secret, base_url, paper)) {
        Ok(session) => {
            tracing::info!(key = %session.masked_key(), paper, "connected to Alpaca");
            state.set_session(session);
        }
        Err(err) => {
            tracing::warn!(%err, "environment credentials rejected; use the UI to connect");
        }
    }
}
