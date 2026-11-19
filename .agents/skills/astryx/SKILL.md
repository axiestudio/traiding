---
name: astryx
description: "Use when building or changing UI with Meta's Astryx design system (@astryxdesign/core) in this repo: choosing components, reading real props from typings, theming, or debugging why an Astryx control renders wrong. Covers the prebuilt-CSS setup, the compound-component patterns this app uses, and the failure modes already hit here."
disable-model-invocation: false
metadata:
  version: 1.0.0
---

# Astryx in traiding

Astryx is Meta's open-source design system (MIT, **Beta**). It is
StyleX-authored but ships **prebuilt CSS + JS**, so there is no build plugin and
no Tailwind. Do not add a second UI library.

## Install shape

```css
/* order matters */
@import '@astryxdesign/core/reset.css';
@import '@astryxdesign/core/astryx.css';
@import '@astryxdesign/theme-neutral/theme.css';
```

```tsx
import { Theme } from '@astryxdesign/core/theme'
import { neutralTheme } from '@astryxdesign/theme-neutral/built'

<Theme theme={neutralTheme} mode="dark"><App /></Theme>
```

`Theme` sets `data-theme` and `data-astryx-theme` on `<html>`. It does **not**
set a page background — see the trap below.

## The rule that matters

**Read the prop from the `.d.ts`, never from memory or a blog post.** Astryx is
Beta and its API moves between patch releases. Every prop in this repo was
verified against:

```
node_modules/@astryxdesign/core/dist/<Component>/<Component>.d.ts
```

There are 116 component entry points; list them with:

```bash
node -e "const p=require('./node_modules/@astryxdesign/core/package.json');console.log(Object.keys(p.exports).filter(k=>k.includes('/')&&!k.endsWith('.css')).join('\n'))"
```

## Traps already hit in this repo

### 1. No body background — the big one

`reset.css` deliberately leaves the page background to the app. Without an
explicit background the webview paints **transparent**: panes show the native
window through them and translucent controls (secondary/destructive buttons)
read as solid white pills with invisible labels.

```css
body {
  background: var(--color-background-body);
  color: var(--color-text-primary);
}
```

### 2. `Canvas` is light-mode only

`color-mix(in srgb, Canvas 78%, transparent)` stays grey in dark mode because
`Canvas` is a light system colour. Use the theme token instead:

```css
background: color-mix(in srgb, var(--color-background-surface) 78%, transparent);
```

### 3. `Dialog` has no `title` prop

Compose it:

```tsx
<Dialog isOpen={open} purpose="required" onOpenChange={() => {}} width={440}>
  <Layout
    header={<DialogHeader title="Connect to Alpaca" />}
    content={<LayoutContent>{/* fields */}</LayoutContent>}
    footer={<LayoutFooter hasDivider>{/* actions */}</LayoutFooter>}
  />
</Dialog>
```

Pass `purpose="required"` and a no-op `onOpenChange` to make it non-dismissable
(the connect dialog must be answered).

### 4. `Table` rows need an index signature

`Table` is data-driven (`columns` + `renderCell` + `idKey`) and its row type is
constrained to `Record<string, unknown>`. Give wire types:

```ts
export interface Position { symbol: string; /* … */ [key: string]: unknown }
```

### 5. `StatusDot` is colour-only

`label` is the dot's `aria-label`; it renders no visible text. Pair it with a
visible label, or the status is conveyed by colour alone (WCAG 1.4.1).

### 6. Sticky headers need the scroll container

Astryx renders the header, but a scrolling pane must pin it to *that* pane:

```css
.blotter__body thead th { position: sticky; top: 0; z-index: 1; }
```

## Components this app uses

| Need | Component | Path |
| --- | --- | --- |
| Buttons | `Button` (`primary`/`secondary`/`ghost`/`destructive`, sizes `sm`/`md`/`lg`) | `@astryxdesign/core/Button` |
| Paper/live, buy/sell, ranges, shares/notional | `SegmentedControl` + `SegmentedControlItem` | `.../SegmentedControl` |
| Order type, TIF | `Selector` (`SelectorOptionData`) | `.../Selector` |
| Positions, orders | `Table` | `.../Table` |
| Positions/Orders switch | `TabList` + `Tab` | `.../TabList` |
| Symbol, key ID, secret | `TextInput` (`value`/`onChange(value)`/`onEnter`/`hasAutoFocus`) | `.../TextInput` |
| Quantity, prices | `NumberInput` (`units` for the `$` suffix) | `.../NumberInput` |
| Badges, status | `Badge`, `StatusDot` | `.../Badge`, `.../StatusDot` |
| Toasts | `useToast()` → `ShowToastFn` | `.../Toast` |
| Connect dialog | `Dialog` + `Layout`/`DialogHeader`/`LayoutContent`/`LayoutFooter` | `.../Dialog` |

`Table` usage:

```tsx
const columns: TableColumn<Row>[] = [
  { key: 'symbol', header: 'Symbol', width: { type: 'pixel', value: 96 } },
  {
    key: 'pl',
    header: 'P/L',
    align: 'end',
    renderCell: (row) => <span className="cell-num">{row.pl}</span>,
  },
]

<Table
  data={rows}
  columns={columns}
  idKey="symbol"
  density="compact"
  dividers="rows"
  hasHover
  textOverflow="truncate"
/>
```

`renderCell` belongs to the **column**, not the `Table`. The row type needs an
index signature (trap 4).

## Theming

Colours come from CSS variables, never hardcoded hex:

```
--color-background-body      --color-background-surface
--color-text-primary         --color-text-secondary
--color-border               --color-accent
--color-success / --color-error (+ -muted, --color-on-*)
```

Values are `light-dark(...)`, so they follow `data-theme` automatically. Keep
app-specific tokens (`--up`, `--down`, `--mono`) in `:root` of `ui/src/styles.css`.
