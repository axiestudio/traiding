# Memory index

Wiki for `traiding`. Short pages, one topic each, kept current. Read the page
you need; do not read them all.

| Page | What it answers |
| --- | --- |
| [architecture.md](architecture.md) | How the pieces fit, why Tauri IPC instead of HTTP |
| [storage.md](storage.md) | SQLite choice, schema, paths, write rules |
| [decisions.md](decisions.md) | Why Astryx, why Lightweight Charts, what was rejected |
| [gotchas.md](gotchas.md) | Bugs already hit here, with symptoms and fixes |
| [glossary.md](glossary.md) | Alpaca + trading terms used in the code |
| [verification.md](verification.md) | How to prove a change renders, in the browser and the real webview |
| [../specs/README.md](../specs/README.md) | Realtime, backtesting and desktop UI specs |

## Conventions

- One fact per line. No prose padding.
- If a page grows past ~80 lines, split it.
- A bug goes in `gotchas.md` **with the symptom**, so the next agent recognises it
  instead of rediscovering it.
- Numbers (sizes, timings, limits) are concrete, never "fast" or "small".

## Current state

- Version 0.1.0, single binary ≈ 7.7 MB stripped (bundled SQLite).
- IPC covers session, account, orders, bars/quotes, streams, backtests and storage.
- IEX quote/trade stream and order-update stream run in Rust behind Tauri events;
  REST polling remains the fallback.
- SQLite schema v2: preferences, market data, snapshots, orders, quotes and
  backtests survive a restart.
- BYOK agent: `src/agent.rs` streams OpenRouter, prepends a Rust-owned
  paper/live prompt and runs 10 read-only tools. Tool cards and run errors
  render inline; the key never reaches the webview.
- Verified 2026-11-19: 1440×900 and 1080×720 browser captures, live-feed badge,
  trade/backtest tabs and a backtest result; agent reply, ordered tool card,
  markdown bold and inline error at both sizes; 29 Rust tests, typecheck green.
- Not built: watchlist, asset autocomplete, intraday/portfolio backtests.
