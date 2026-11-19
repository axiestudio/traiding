# Gotchas

Bugs already hit here. Each entry gives the **symptom** so you recognise it fast.

## 1. `missing field 'keyId'` on connect

**Symptom:** clicking Connect shows `invalid args 'req' for command 'connect': missing field 'keyId'`.

**Cause:** `ConnectReq` has `#[serde(rename_all = "camelCase")]`, so the wire name
is `keyId`, but `api.ts` sent `key_id`.

**Fix:** `ui/src/api.ts` must send `{ req: { keyId, secret, mode } }`.

**Guard:** `commands::tests::connect_req_accepts_the_payload_the_ui_sends` pins the
exact JSON. When you change a wire struct, update that test in the same commit.

## 2. Page paints transparent — white "blank" buttons

**Symptom:** panes show the desktop through them; secondary and destructive
buttons look like solid white pills with no label.

**Cause:** Astryx's `reset.css` deliberately does not set a page background. With
no `body` background the webview is transparent, and semi-transparent button
fills composite against white.

**Fix:** `body { background: var(--color-background-body); color: var(--color-text-primary); }`

**Trap:** it can look like a button bug. Check `document.body`'s painted alpha
first — sample a screenshot pixel, not just `getComputedStyle`.

## 3. `Canvas` colour is light-mode only

**Symptom:** a surface stays light grey in dark mode.

**Cause:** `color-mix(in srgb, Canvas 78%, transparent)` — `Canvas` is a system
colour pinned to light mode.

**Fix:** use the theme token: `var(--color-background-surface)`.

## 4. `Dialog` has no `title` prop

**Symptom:** TypeScript error, or a dialog with no visible heading.

**Fix:** compose `<Dialog><Layout header={<DialogHeader title="…" />} …>`.

## 5. `Table` rows rejected by TypeScript

**Symptom:** `Type 'Position' does not satisfy the constraint 'Record<string, unknown>'`.

**Fix:** add `[key: string]: unknown` to the wire interface. Astryx's `Table` needs
an index signature.

## 6. Vite 8: `transformWithEsbuild` deprecated

**Symptom:** `Failed to load 'transformWithEsbuild'. It is now deprecated and it
now requires esbuild to be installed separately`.

**Fix:** `build.minify: 'oxc'` (Rolldown's native minifier), not `'esbuild'`.

## 7. `tauri build` from `ui/` resolves `ui/ui/package.json`

**Symptom:** `npm error path …/traiding/ui/ui/package.json`.

**Cause:** `npx --prefix ui tauri build` changes cwd into `ui/`, so the relative
prefix doubles.

**Fix:** run Tauri from the **repo root**. The root `package.json` owns
`@tauri-apps/cli`; config uses `npm run build --prefix ui`.

## 8. `generate_context!` panics on a missing icon

**Symptom:** build fails with `failed to open icon …/icons/32x32.png`.

**Cause:** `tauri.conf.json` lists four icon paths and all must exist.

**Fix:** keep `icons/32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.png`.
The set was generated with a PIL script; no ImageMagick needed.

## 9. Screenshots of `:0` come back all-black

**Symptom:** `xwd`/`ffmpeg` captures of the desktop are uniformly black;
`xdotool search` only finds a "mutter guard window".

**Cause:** the session is Wayland/mutter, so the Tauri window is not mapped in
the `:0` X11 display.

**Fix:** start a private `Xvfb :99`, run the app with `DISPLAY=:99` and
`GDK_BACKEND=x11`, then capture `:99`. See [verification.md](verification.md).
The chart is canvas — always look at a screenshot, not a DOM dump.

## 10. `npm run typecheck` at the root: missing script

**Symptom:** `npm error Missing script: "typecheck"`.

**Cause:** the root script delegates to `ui/`, which lacked one.

**Fix:** `ui/package.json` has `"typecheck": "tsc --noEmit"`.

## 11. Chart collapses at 1080×720

**Symptom:** at the window minimum the chart is ~28px tall and its header
overlaps the blotter tabs; the ticket is cut off.

**Cause:** a `max-width: 1080px` rule stacked chart, 220px blotter and 420px
ticket into a 720px window.

**Fix:** keep the fixed two-column grid at every size the window can reach; the
minimum is 1080×720. Guard with a screenshot at exactly 1080×720.

## 12. Volume pane ignores `setHeight(72)`

**Symptom:** the histogram takes roughly a third of the chart instead of a thin
strip, even though the call is present. A one-time `requestAnimationFrame` call
is not enough once `autoSize` resizes the container.

**Cause:** a pane height is stored as a stretch factor. `autoSize` recalculates
all pane heights on every container resize, so the requested 72 px is not
stable.

**Fix:** re-apply it after every resize. `PriceChart` observes its container and
calls `chart.panes()[1]?.setHeight(72)` in a frame after the chart's own
ResizeObserver, with both frames cancelled on cleanup. Do not put volume on an
overlay scale: the price axis keeps its full range and the volume bars cover the
bottom price labels. Do not hide the volume price scale either — `visible: false`
collapsed the chart to an empty canvas when tried here.

## 13. AppImage bundling: `Text file busy (os error 26)`

**Symptom:** `npm run build` ends with `failed to bundle project: Text file busy
(os error 26)`; the AppImage is oversized or partial.

**Cause:** the release binary was running while Tauri patched and packaged it —
usually because a native Xvfb check was started during the build.

**Fix:** build first, then launch the binary. Stop the app before rebuilding or
re-bundling.

## 14. Saved symbol keeps resetting to AAPL

**Symptom:** the app restores defaults on every launch even though `settings`
contains the previous `ui_state`.

**Cause:** the save effect in `App.tsx` ran on the first render and wrote
`AAPL`/`6M` before `get_ui_state` resolved, overwriting the saved row.

**Fix:** gate writes on the `prefsReady` state, set only after a successful
`get_ui_state`. Use state, not a ref: a ref does not re-run the effect, so the
initial hydration never writes the defaults.

## 15. `local storage is unavailable`

**Symptom:** every preference and snapshot call rejects, but the dashboard works.

**Cause:** `Storage::open` failed during setup (permissions, corrupt file,
read-only home). `lib.rs` logs a warning and continues without a database by
design.

**Fix:** check the startup log for the `local storage` warning and the path.
Repair permissions or delete the `storage` folder; do not make storage a hard
startup dependency.

## 16. `failed to run linuxdeploy` on AppImage

**Symptom:** `npm run build` finishes `deb` and then dies on the AppImage half
with `failed to bundle project: failed to run linuxdeploy`. Re-running the same
command succeeds.

**Cause:** the bundled `linuxdeploy` step is flaky when the release binary was
just relinked or a previous AppDir is still on disk — same class as gotcha 13.

**Fix:** rebuild just that target; it is not a config error.

```bash
npx tauri build --bundles appimage
```

A single-bundle build **wipes** the other `target/release/bundle/*` folders, so
rebuild them before quoting bundle contents:

```bash
npx tauri build --bundles deb,rpm
```

**Guard:** after any bundle change, confirm the sidecar really landed.

```bash
dpkg-deb -c target/release/bundle/deb/*.deb | grep alpaca
find target/release/bundle/appimage -name '*alpaca*'
```

## 17. Chart jumps on every live trade

**Symptom:** during market hours the chart scrolls or rescales on each tick,
making the crosshair unusable.

**Cause:** the live trade handler replaced the last bar array, and `PriceChart`
called `chart.timeScale().fitContent()` on every `bars` effect.

**Fix:** fit only when the bar window signature (first time, last time, length)
changes. Live ticks mutate the last close/high/low in place.

## 18. Feed badge says `feed polling` forever

**Symptom:** the top bar never reaches `feed live` even though quotes update.

**Cause:** the Rust stream emits `stream:status` once on connect; a listener
registered after that event missed it, or the event plugin was not reachable.

**Fix:** start streams through `set_stream_symbol` after the UI session is
connected and register the event listeners in the same connected effect. The
market loop emits `connecting` then `live` for each new symbol subscription.

## 19. Backtest result shows the previous symbol

**Symptom:** switching from AAPL to MSFT leaves AAPL's return under the MSFT
header.

**Cause:** the result lived only in `BacktestPanel` local state and survived the
symbol prop change.

**Fix:** clear `result` and `error` in a `useEffect` keyed on `symbol`.

## 20. SQLite schema v1 has no `backtests` table

**Symptom:** `run_backtest` succeeds but `no such table: backtests` is logged.

**Cause:** `PRAGMA user_version` is 1 and only the old tables were created.

**Fix:** bump `SCHEMA_VERSION` to 2; the idempotent `CREATE ... IF NOT EXISTS`
batch runs on the next open and creates `backtests`. The command still returns
the result if the write fails (best-effort storage).

## 21. `externalBin` puts the CLI on every user's `PATH`

**Symptom:** installing the `deb`/`rpm` adds `/usr/bin/alpaca` next to the app.

**Cause:** Tauri's `bundle.externalBin` installs sidecars to `usr/bin` by
convention. There is no per-target override.

**Fix:** use `bundle.resources` with a mapped destination instead:
`{ "binaries/alpaca": "bin/alpaca" }` lands at `usr/lib/traiding/bin/alpaca`.
Resolve it in Rust with `AppHandle::path().resource_dir()`, never from `PATH`.
Resource names take no target triple, so `binaries/alpaca` — not
`binaries/alpaca-<triple>`.

## 22. Agent pane collapsed to its content height

**Symptom:** the composer floated mid-pane and the empty state hugged the top.

**Cause:** the agent sat inside a bare `<div>` between `.rail__body` and
`.agent-panel`. `height: 100%` on the panel had no resolved height to inherit.

**Fix:** give the wrapper `.rail__pane { height: 100% }`, and let the empty
state take `flex: 1` so it centres.

## 23. Ling 3.2 Flash streams `reasoning` and no `content`

**Symptom:** SSE returns frames but `content` is `""`; the pane looks frozen
and a small `max_tokens` never reaches an answer.

**Cause:** it is a hybrid-reasoning model. Thinking tokens arrive as
`delta.reasoning` until the budget runs out.

**Fix:** send `"reasoning": { "enabled": false }`. Opt back in with
`AGENT_REASONING=1` when the reasoning text is wanted.

## 24. Tool errors never show as errors

**Symptom:** a failed tool call renders in the normal style; the red error
border never appears, even though the model sees `isError` in its history.

**Cause:** `StreamEvent` is an enum, and `#[serde(rename_all = "camelCase")]`
renames variants only — not their fields. `ToolDone.is_error` reached the
webview as `is_error`, while `ipc.ts` reads `isError`, so the flag was always
`undefined`.

**Fix:** add `rename_all_fields = "camelCase"` to the enum. Guard:
`agent::tests::stream_events_serialize_camel_case` pins every frame shape.

## 26. Chart rejects a valid paper key

**Symptom:** `Alpaca rejected the credentials: subscription does not permit
querying recent SIP data` fills the chart on a free plan.

**Cause:** bars requests omitted `feed`, so Alpaca used SIP.

**Fix:** set `feed: Some(Feed::IEX)` in every `bars::ListReqInit` (chart and
backtester). IEX works on every plan; SIP is paid.

## 27. Feed polls for minutes and quotes look 15 minutes old

**Symptom:** the top bar stays `feed polling`; the chart ends ~15 minutes in the
past even with a valid free key.

**Cause:** Basic historical REST is limited to data older than 15 minutes; only
the IEX websocket is real-time. A second process holds the single market-data
connection (406), so a stale instance can keep the first from live data.

**Fix:** the single-instance plugin; stream trades update the last candle and the
header price; REST quote polling stops while the market socket is live. Latest
quotes request `feed=iex`.

## 25. Agent failure looks like an empty bubble

**Symptom:** a bad key, rate limit or provider outage ends the turn, but the
thread shows nothing and the pane looks stuck.

**Cause:** assistant-ui stores the run error on the message status, and
`AgentMessage` rendered `MessagePrimitive.Parts` only.

**Fix:** render `MessagePrimitive.Error` and read `status.error` through
`useAuiState`. `?mock=agent-error` exercises the path without a key.
