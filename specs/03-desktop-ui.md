# Desktop UI

Status: accepted for implementation.

## Product

A single native window, dense enough for a trading desk at 1080×720 and calm at
1440×900. Astryx is the only control system; Lightweight Charts v5 is the only
chart engine. The window is the app: no browser chrome, no server, no separate
web deployment.

## Layout

```
┌ top bar: brand · paper/live · market clock · equity · buying power · disconnect ┐
├───────────────────────────────────────────────────────┬────────────────────────┤
│ chart: symbol · last · change · range                 │ Trade | Backtest       │
│ quote strip: bid ask mid IEX live time                │                        │
│ candlesticks + volume                                 │ order ticket           │
├───────────────────────────────────────────────────────┤ or backtest form       │
│ blotter: Positions | Orders                           │ and results            │
└───────────────────────────────────────────────────────┴────────────────────────┘
```

- Grid: `1fr 340px`, rows `1fr 248px`. The right rail is one pane with tabs.
- At 1080×720 the chart remains the visual anchor; the right rail scrolls
  internally and never pushes the blotter off-screen.
- No horizontal scroll at the minimum size. Tables truncate; numbers stay tabular.

## Chart

- Candlesticks, volume in a 72 px lower pane, crosshair legend with OHLCV.
- Sticky visible range is not used: a symbol/range change fits content once.
- Realtime updates the last candle in place and never calls `fitContent` on every
  tick, or the chart would jump under the cursor.
- The quote strip is a quiet monospace row. Bid green, ask red, mid neutral.
- `IEX live` carries a visible dot and text; `IEX` is the fallback label.

## Right rail

- Tab switch is a compact segmented control at the top of the rail. Trade and
  Backtest are specified here; the chat feature adds a third Agent tab.
- **Trade** keeps the existing ticket, price estimate, buying power and held
  quantity. Labels stay ≤ 2 words.
- **Backtest** fits in the same rail: inputs, one primary action, metrics and a
  150 px equity chart. It must not need a modal.
- Switching tabs does not lose ticket state or a completed backtest result.

## Feed feedback

- App-level state: `market` and `orders` each `connecting | live | offline`.
- Top bar shows `feed live` with a dot when either stream is live; `feed polling`
  before the first event or after a failure. Text, not colour alone.
- Order events refresh the blotter; no full-page spinner.

## Accessibility and interaction

- Every icon-only control has an accessible label. Status is never colour-only.
- Ctrl/Cmd+K focuses the symbol field, as before.
- Buttons are disabled while their request is in flight; errors appear inline,
  never as a bare alert.
- Escape closes no trading surface because none is modal; the connect dialog
  remains the only required dialog.

## Copy budget

Labels ≤ 2 words, helper text ≤ 12 words. New strings:

`Trade`, `Backtest`, `feed live`, `feed polling`, `Range`, `Fast`, `Slow`,
`Starting cash`, `Run backtest`, `Return`, `Benchmark`, `Max drawdown`,
`Win rate`, `Trades`, `Equity`, `No runs yet`, `Running…`.

## Acceptance

1. 1440×900: rail tabs, chart, quote strip and blotter all visible without
   scrolling the page.
2. 1080×720: no clipped controls; rail scrolls internally.
3. Backtest result renders at both sizes.
4. Connect dialog still opens when disconnected and cannot be dismissed.
5. `npm run typecheck` is clean; screenshots reviewed at both sizes.
