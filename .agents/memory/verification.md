# Verification

What to run before calling a change done. Checks first, then rendering.

## Gate

```bash
cargo check --all-targets     # zero warnings
cargo test                    # 33 Rust tests, 8 of them agent
npm run typecheck             # tsc --noEmit
npm run build                 # UI + Tauri release bundle
```

A green build does not prove rendering. Continue.

## UI with fake data, no keys

`npm --prefix ui run dev` serves the UI in a plain browser. With no Tauri shell
present, `main.tsx` installs `ui/src/dev/mockTauri.ts`, which fakes
`window.__TAURI_INTERNALS__`. It is behind `import.meta.env.DEV` and a dynamic
import, so the production bundle drops it.

```bash
npm --prefix ui run dev
agent-browser open http://localhost:1420
agent-browser set viewport 1440 900
agent-browser screenshot /tmp/traiding-1440.png
agent-browser set viewport 1080 720
agent-browser screenshot /tmp/traiding-1080.png
# click Backtest, run it, then capture the result at both sizes
```

- `?mock=connect` shows the connect dialog; `?mock=live` uses live styling.
- Screenshot, never just a DOM snapshot: the chart is canvas.
- 1080×720 is the window minimum. The grid must stay two-column; the chart needs
  real height. Stacking panes at this size collapses it (gotcha 11).

What to look for: page background painted (not transparent), thin volume strip
(target 72 px), numbers tabular and aligned, ticket fully visible at 900, no
clipped copy.

## Storage

The database is created on first native launch. With the app stopped:

```bash
DB=${XDG_DATA_HOME:-$HOME/.local/share}/dev.traiding.terminal/storage/traiding.db
sqlite3 "$DB" '.tables' 'PRAGMA user_version'
```

Use `XDG_DATA_HOME=/tmp/traiding-xdg` when launching so a verification run
never touches real preferences or history. Use `XDG_CONFIG_HOME=/tmp/traiding-config`
as well so a test connect cannot overwrite the real credential `.env`.

Expect `settings bars account_snapshots position_snapshots orders quotes backtests`
and version `2`. Change symbol/range/tab, relaunch, and confirm they are restored.
Run a backtest and confirm the row count rises. Delete the `storage` folder to
reset. A database failure must only warn; live data still renders.

This host exports `AGENT_BROWSER_ENGINE=obscura` **and**
`AGENT_BROWSER_EXECUTABLE_PATH=…/obscura`; the executable path wins over
`--engine chrome` and rejects `--remote-debugging-port`. Unset it and point at
Chrome for Testing explicitly:

```bash
unset AGENT_BROWSER_EXECUTABLE_PATH
CHROME=$(ls -d "$HOME"/.agent-browser/browsers/chrome-*/chrome | head -1)
agent-browser --engine chrome --executable-path "$CHROME" \
  --args --no-sandbox --session traiding-ui open http://localhost:1420
```

Repeat the engine/executable/args flags on every command; a restarted daemon
otherwise falls back to Obscura and blocks `localhost`.

## Native webview

```bash
npm run build
mkdir -p /tmp/traiding-native /tmp/traiding-xdg && cd /tmp/traiding-native
Xvfb :99 -screen 0 1440x900x24 &
DISPLAY=:99 GDK_BACKEND=x11 WEBKIT_DISABLE_COMPOSITING_MODE=1 \
  XDG_DATA_HOME=/tmp/traiding-xdg XDG_CONFIG_HOME=/tmp/traiding-config \
  /home/stefan/Projects/traiding/target/release/traiding &
sleep 4
DISPLAY=:99 ffmpeg -y -loglevel error -f x11grab \
  -video_size 1440x900 -i :99 -frames:v 1 /tmp/traiding-native.png
```

Run from a directory without `.env` to see the connect dialog without touching
credentials. Do not capture `:0`: the desktop is Wayland and the Tauri window is
not mapped there (gotcha 9). Xvfb is a private X server, so it is safe.

Build **before** launching: packaging patches the release binary, and a running
copy makes AppImage bundling fail with `Text file busy` (gotcha 13).

## Credentials

Run from a directory without `.env`, with both `XDG_DATA_HOME` and
`XDG_CONFIG_HOME` pointed at temp directories. Enter the paper key pair; after
Connect, confirm the file exists and is private, without printing the values:

```bash
CONFIG=${XDG_CONFIG_HOME:-$HOME/.config}/dev.traiding.terminal/.env
stat -c '%a' "$CONFIG"          # 600
cut -d= -f1 "$CONFIG" | grep -E '^APCA_(API_KEY_ID|API_SECRET_KEY|API_BASE_URL)$'
```

Relaunch from the same temp config with no keys exported; it must connect
without the dialog. The database must still contain no secret.

## Interactions

- Orders tab: sentence-case badges; Cancel only on open orders.
- Ticket: Limit reveals Limit price; Stop reveals Stop price; Stop limit both.
- Enter a `PK…` key, select Live: the mismatch appears under Key ID.
- Disconnect: the connect dialog returns, dashboard values blank.

## Last verified

2026-11-19 — v0.2.0 AppImage installed at `~/.local/share/traiding`. Fresh
`XDG_CONFIG_HOME`/`XDG_DATA_HOME`: the connect dialog saved the Alpaca pair to
`.env` at 0600, a relaunch auto-connected from it, and the Agent key was merged
into the same file without losing the Alpaca keys. NVDA/6M was restored from
SQLite after a relaunch (`ui_state`, 124 bars, `integrity_check ok`). A
pre-market IEX quote at 08:33 ET drove the header, quote strip and `feed live`.
`cargo check`, 33 tests and `npm run typecheck` green. Browser-mock captures
from the same day cover both window sizes.
