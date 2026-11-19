//! `traiding` — an Alpaca paper/live trading terminal as a desktop app.
//!
//! The whole product is one process: Tauri opens a native window, the React UI
//! runs inside that webview, and every data request is an in-process IPC call
//! into the `apca` client. Nothing listens on a TCP port, so there is no
//! browser, no CORS surface and no separate static-asset deployment.

fn main() {
    traiding_lib::run()
}
