/**
 * Small formatters for the agent surface.
 *
 * Kept separate from `api.ts` because these handle model payloads, not the
 * Alpaca wire types the panes use.
 */

/** One-line JSON preview: `{"symbol":"AAPL"}` rather than a pretty block. */
export function fmtJson(raw: unknown, max = 90): string {
  const text = typeof raw === 'string' ? raw : JSON.stringify(raw)
  if (!text) return ''

  let compact = text.replace(/\s+/g, ' ')
  try {
    compact = JSON.stringify(typeof raw === 'string' ? JSON.parse(raw) : raw)
  } catch {
    // A partial argument string mid-stream: show it as-is.
  }

  return compact.length > max ? `${compact.slice(0, max - 1)}…` : compact
}

/** Readable tool names: `get_quote` -> `Get quote`. */
export function toolLabel(name: string): string {
  const spaced = name.replace(/_/g, ' ').trim()
  return spaced.charAt(0).toUpperCase() + spaced.slice(1)
}
