---
name: copywriting
description: "Use when writing or editing any user-facing text in traiding: button labels, dialog copy, empty states, error messages, tooltips, or README prose. Enforces short, plain, trading-native wording and lists the exact strings this app ships."
disable-model-invocation: false
metadata:
  version: 1.0.0
---

# Copywriting for traiding

A trading terminal is read at a glance, under time pressure, next to live money.
Every word competes with the numbers. **Shorter is not a style preference here —
it is the feature.**

## The rules

1. **Cut the sentence in half, then cut it again.** If a line explains what the
   control obviously does, delete it.
2. **Label the action, not the mechanism.** "Close", not "Liquidate position at
   market". "Cancel", not "Cancel this order".
3. **No hedging.** Kill "please", "simply", "just", "in order to", "make sure
   that", "note that".
4. **Numbers before prose.** `$41,822.55`, not "your equity is $41,822.55".
5. **Errors state the cause and the next step.** Nothing else. No apologies, no
   "Oops".
6. **Sentence case.** Not Title Case, not ALL CAPS. Labels may be uppercase only
   as CSS `text-transform`, never in the source string.
7. **No exclamation marks.** A trading app never sounds excited.
8. **Say "live" and mean it.** Live mode risks real money — that is the one place
   to spend words.

## Before / after

| Before | After |
| --- | --- |
| "Please enter your API key ID in the field below" | "Key ID" |
| "Your position has been successfully closed" | "Position closed" |
| "Orders route to paper trading. Credentials stay in this Rust process only." | "Paper trading. Keys are never saved." |
| "Are you sure you want to cancel this order?" | "Cancel order?" |
| "No open positions were found in your account" | "No open positions" |
| "Failed to load chart data for the selected symbol" | "No bars for AAPL" |
| "An unexpected error occurred. Please try again later." | "Request failed. Try again." |
| "This will place a real order using real money" | "Live trading. Real money." |

Budget: **labels ≤ 2 words, helper text ≤ 12 words, errors ≤ 20 words.** If a
string cannot fit, the design is unclear — fix the design, not the copy length.

## Shipped strings

Keep these exact; they are already tightened. Match their register.

**Connect dialog** — `Connect to Alpaca` / `Paper` / `Live` / `Key ID` / `Secret`
/ `Connect` / `Get keys from your Alpaca dashboard.` / `PK is paper, AK is live.`
/ `Shown once when created.` / `Lost it? Regenerate in Alpaca.` / `Paper key in
live mode. Switch mode.` / `Live trading. Real money.` / `Keys are never saved.`

`Keys are never saved.` is the one security claim. Keep it.

**Top bar** — `Equity` / `Buying power` / `Disconnect` / `market open` /
`market closed` / `market unknown` / `paper` / `live` / `feed live` /
`feed polling`

**Blotter** — `Positions` / `Orders` / `Close` / `Cancel` / `No open positions` /
`No orders` / `Symbol` / `Qty` / `Avg entry` / `Last` / `Value` / `P/L` /
`Side` / `Type` / `Filled` / `Avg price` / `Status`

**Ticket** — `Order ticket` / `Buy` / `Sell` / `Shares` / `Notional` /
`Quantity` / `Type` / `TIF` / `Limit` / `Stop` / `Day` / `GTC` / `IOC` / `FOK` /
`Est. cost` / `Est. proceeds` / `Buying power` / `Held` / `Buy AAPL` /
`Paper trading · keys are never saved.`

**Rail / backtest** — `Trade` / `Backtest` / `Range` / `Fast` / `Slow` /
`Starting cash` / `Run backtest` / `Running…` / `Return` / `Benchmark` /
`Max drawdown` / `Win rate` / `Trades` / `Equity` / `SMA crossover` /
`Strategy` / `Buy & hold` / `No runs yet` / `Open`

**States** — `Loading AAPL…` / `No bars for AAPL` / `Order submitted` /
`Order canceled` / `Order filled` / `Order rejected` / `Order expired` /
`Close order sent` / `Connected: paper` / `hover for OHLCV` / `IEX live`

**Errors** — `Enter a quantity.` / `Enter a notional.` / `Enter a limit price.` /
`Enter a stop price.` / `Enter starting cash.` / `Fast must be below slow.` /
`Request failed.`

## Errors

`src/error.rs` already unwraps Alpaca's own message — show it verbatim rather
than wrapping it. Do not prefix with "Error:". One sentence, then the fix:

```
insufficient buying power
symbol not found
keys rejected — reconnect
that looks like the API endpoint, not the key ID
```

## Where copy lives

- UI strings: in the component, as plain JSX text. No i18n layer yet.
- Rust messages: `src/error.rs` and `bad_request(...)` call sites.
- Docs: `README.md` — same discipline, but full sentences are allowed there.

When you change a string, check it is not asserted in `commands::tests`, update
the shipped strings above, and re-read the screenshot at 1440×900. Backtest
labels are constrained by the 340 px rail: `Max drawdown` must fit one line at
9 px uppercase; use `Trades` not a helper sentence.
