# Specs

Implementation specs for the realtime + backtesting release. Read in order.

| Spec | Scope |
| --- | --- |
| [01-realtime.md](01-realtime.md) | IEX quote/trade feed, order-update socket, lifecycle and events |
| [02-backtesting.md](02-backtesting.md) | SMA crossover engine, IPC shape, SQLite storage and UI |
| [03-desktop-ui.md](03-desktop-ui.md) | Layout, right-rail tabs, feed feedback, responsive acceptance |

Non-negotiables still apply: one process, no credentials in the webview or
SQLite, camelCase wire names, Astryx props from typings, zero warnings. Alpaca
keys persist only in the app-config `.env` at mode 0600.
