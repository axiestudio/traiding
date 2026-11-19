/**
 * Symbol selector.
 *
 * Clicking shows the liquid names without typing; typing searches the full
 * tradable asset list through the backend. The chart keeps the chosen symbol.
 */

import { useEffect, useRef, useState, type KeyboardEvent } from 'react'

import { api, type AssetRow } from '../api'

const POPULAR = ['AAPL', 'NVDA', 'MSFT', 'TSLA', 'SPY', 'QQQ', 'AMZN', 'META', 'GOOGL', 'AMD']

interface SymbolPickerProps {
  value: string
  onChange: (symbol: string) => void
}

export function SymbolPicker({ value, onChange }: SymbolPickerProps) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [assets, setAssets] = useState<AssetRow[]>([])
  const [loading, setLoading] = useState(false)
  const [active, setActive] = useState(0)

  const rootRef = useRef<HTMLDivElement | null>(null)
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const searchRef = useRef<HTMLInputElement | null>(null)

  const rows: AssetRow[] =
    query.trim() === ''
      ? POPULAR.map((symbol) => ({
          symbol,
          exchange: '',
          class: '',
          tradable: true,
          fractionable: false,
          shortable: false,
          easy_to_borrow: false,
        }))
      : assets

  // Fetch when the query changes. The empty query is the static shortlist, so
  // opening the picker never waits on a network round trip.
  useEffect(() => {
    if (!open || query.trim() === '') return

    let cancelled = false
    const needle = query.trim().toUpperCase()
    setLoading(true)
    const timer = window.setTimeout(() => {
      api
        .listAssets(needle, 25)
        .then((page) => {
          if (!cancelled) {
            setAssets(page.assets.filter((asset) => asset.tradable))
            setActive(0)
          }
        })
        .catch(() => {
          if (!cancelled) setAssets([])
        })
        .finally(() => {
          if (!cancelled) setLoading(false)
        })
    }, 140)

    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [open, query])

  useEffect(() => {
    if (!open) return
    searchRef.current?.focus()
  }, [open])

  useEffect(() => {
    if (!open) return
    const onDown = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', onDown)
    return () => document.removeEventListener('mousedown', onDown)
  }, [open])

  function close(focus = true) {
    setOpen(false)
    setQuery('')
    if (focus) triggerRef.current?.focus()
  }

  function choose(symbol: string) {
    if (symbol !== value) onChange(symbol)
    close()
  }

  function onSearchKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      setActive((index) => Math.min(index + 1, rows.length - 1))
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      setActive((index) => Math.max(index - 1, 0))
    } else if (event.key === 'Enter') {
      event.preventDefault()
      const row = rows[active]
      if (row) choose(row.symbol)
    } else if (event.key === 'Escape') {
      event.preventDefault()
      close()
    }
  }

  return (
    <div className="symbol-picker" ref={rootRef}>
      <button
        ref={triggerRef}
        type="button"
        className="symbol-picker__trigger"
        data-symbol-picker
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={`Symbol ${value}`}
        onClick={() => setOpen((isOpen) => !isOpen)}
      >
        <span className="symbol-picker__value">{value}</span>
        <span className="symbol-picker__chevron" aria-hidden="true" />
      </button>

      {open ? (
        <div className="symbol-picker__menu">
          <input
            ref={searchRef}
            type="search"
            className="symbol-picker__search"
            placeholder="Search symbol"
            aria-label="Search symbol"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value)
              setActive(0)
            }}
            onKeyDown={onSearchKeyDown}
          />
          <div className="symbol-picker__list" role="listbox" aria-label="Symbols">
            {rows.map((asset, index) => (
              <button
                key={asset.symbol}
                type="button"
                role="option"
                aria-selected={asset.symbol === value}
                className={`symbol-picker__option${index === active ? ' is-active' : ''}`}
                onMouseEnter={() => setActive(index)}
                onClick={() => choose(asset.symbol)}
              >
                <span className="symbol-picker__symbol">{asset.symbol}</span>
                {asset.exchange ? (
                  <span className="symbol-picker__meta">{asset.exchange}</span>
                ) : null}
              </button>
            ))}
            {!loading && query.trim() !== '' && rows.length === 0 ? (
              <p className="symbol-picker__empty">No match</p>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  )
}
