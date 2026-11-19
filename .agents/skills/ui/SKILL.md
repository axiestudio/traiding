---
name: ui
description: "Use when changing the React frontend of traiding: adding a pane, changing the layout grid, wiring a new IPC call, polling, chart work with Lightweight Charts, or fixing something that renders wrong. Covers the component split, the single IPC boundary, and how to verify a change actually renders."
disable-model-invocation: false
metadata:
  version: 1.0.0
---

# UI side of traiding

React 19 + TypeScript (strict) + Vite 8, built into the binary and served from
the Tauri asset store. No dev server at runtime, no HTTP.

## Shape

```
src/main.tsx        Theme wrapper + styles.css
src/App.tsx         owns session, polling, symbol/range, saved preferences
src/api.ts          THE IPC boundary — every invoke/listen lives here
src/styles.css      only what a design system cannot know
src/components/     TopBar ConnectDialog ChartPanel PriceChart
                    PositionsTable OrdersTable OrderTicket Blotter
                    BacktestPanel EquityChart
```

## Rules

**All IPC goes through `src/api.ts`.** Components receive data and callbacks as
props; they never call `invoke` themselves. That keeps polling policy in `App.tsx`
and makes the wire shape auditable in one file.

**Guard against out-of-order responses.** Fast typing in the symbol box fires
overlapping fetches; `App.tsx` uses a monotonically increasing
`chartRequestRef` and drops any response whose id is stale. Any new
request-on-input path needs the same guard.

**Realtime is event-driven.** `App.tsx` listens to `market:quote`,
`market:trade`, `trade:update` and `stream:status` through `api.onEvent`; never
import `@tauri-apps/api/event` in a component. Quote/trade state is throttled to
4 Hz. The trade handler mutates the last bar's close/high/low; do not refetch the
whole chart for a tick.

**Keep the polling cadence.** Overview every 15 s, quote every 20 s — this is
deliberate rate-limit headroom and the fallback when a socket drops. Do not lower
it without checking Alpaca's limits.

The right rail is a `Trade | Backtest | Agent` segmented control; all three panes
live in the same 340 px column and the rail body scrolls at 1080×720.

**Preferences are debounced.** `App.tsx` reads `get_ui_state` on mount, then a
500 ms effect saves symbol/range/tab. Gate saves with the `prefsReady` state;
without it the first render writes defaults over the saved row (gotcha 14).

**Styles:** Astryx supplies all chrome. `styles.css` holds only the terminal grid,
monospace tabular numerics, and the green/red P&L vocabulary. Reuse
`--up`/`--down`/`--mono`/`--hairline` rather than inventing colours. Never set a
raw hex that duplicates a theme token. See the `astryx` skill for the token list.

**Formatting:** use `num`, `fmtMoney`, `fmtNum`, `fmtQty`, `signed` and
`sentenceCase` from `api.ts` so every number and enum renders identically.
`fmtQty` trims trailing zeroes (120, not 120.0000); `signed()` uses a real minus
sign (U+2212) so gains and losses align.

## Chart

`PriceChart.tsx` wraps TradingView Lightweight Charts v5. That is a **v5** API:

```ts
const chart = createChart(container, { autoSize: true, … })
const candles = chart.addSeries(CandlestickSeries, opts)   // not addCandlestickSeries
const volume = chart.addSeries(HistogramSeries, opts, 1)    // pane index 1
const keepVolumeThin = () => chart.panes()[1]?.setHeight(72)
requestAnimationFrame(keepVolumeThin)
new ResizeObserver(() => requestAnimationFrame(keepVolumeThin)).observe(container)
```

`setHeight` is a stretch factor, and `autoSize` recalculates it on every resize;
re-apply after resizing (gotcha 12). Do not use an overlay price scale for
volume — the price axis then spans the histogram and its labels are covered —
and do not hide the volume scale with `visible: false`; it blanked the chart.

Bar `time` is a **Unix timestamp in seconds**, matching `src/commands.rs`. Always
`chart.remove()` on unmount, or the ResizeObserver leaks across HMR reloads.

Live ticks replace only the last bar. `PriceChart` fits content only when the
bar window signature changes (first time, last time, length); fitting on every
tick drags the chart under the crosshair. `EquityChart` is the same v5 pattern
with two `LineSeries` (strategy green, benchmark dashed grey) and a 150 px box.

## Verify a change — do not trust the build alone

`vite build` passing proves nothing about rendering. When Vite serves the UI
without Tauri, `main.tsx` installs the committed mock in
`ui/src/dev/mockTauri.ts`, so the populated UI can be screenshotted without
keys:

```bash
npm --prefix ui run dev
agent-browser open http://localhost:1420
agent-browser set viewport 1440 900
agent-browser screenshot /tmp/traiding-1440.png
agent-browser set viewport 1080 720      # window minimum
agent-browser screenshot /tmp/traiding-1080.png
```

Then open `http://localhost:1420/?mock=connect` for the connect dialog and
`?mock=live` for live styling. Click **Backtest**, run it, and capture the result
at both sizes. Check **screenshots**, not DOM snapshots — the chart is canvas, and
the layout only fails visually. `ui/src/dev/mockTauri.ts` is loaded by a
DEV-only dynamic import and never ships. It stubs `connect`, `get_ui_state`,
`save_ui_state`, `run_backtest`, `set_stream_symbol` and the event plugin so the
connect flow, feed badge and result render without keys.

The native webview is checked separately under Xvfb; see
[`memory/verification.md`](../../memory/verification.md). Two rendering bugs
were found this way and are documented traps in the `astryx` skill: a
transparent page background, and light-mode `Canvas` colours in dark mode.

## Checks

```bash
npm run typecheck        # tsc --noEmit, strict
npm run build            # typecheck + vite build
```

`tsconfig` has `noUnusedLocals`, `noUnusedParameters` and
`verbatimModuleSyntax` — use `import type { … }` for type-only imports.
