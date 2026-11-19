/**
 * Order ticket — the only place in the app that can spend money.
 *
 * The estimate block is a rough guide, not a guarantee: it uses the last bar
 * close for market orders and the typed limit price otherwise, so the user can
 * see roughly what they are about to commit before pressing the button.
 */

import { useState } from 'react'
import { Button } from '@astryxdesign/core/Button'
import { NumberInput } from '@astryxdesign/core/NumberInput'
import { SegmentedControl } from '@astryxdesign/core/SegmentedControl'
import { SegmentedControlItem } from '@astryxdesign/core/SegmentedControl'
import { Selector } from '@astryxdesign/core/Selector'

import { api, apiError, fmtMoney, num } from '../api'
import type { Account, CreateOrderRequest, Order, Position } from '../api'

export type OrderType = 'market' | 'limit' | 'stop' | 'stop_limit'
export type TimeInForce = 'day' | 'gtc' | 'ioc' | 'fok'
export type SizeMode = 'qty' | 'notional'

interface OrderTicketProps {
  symbol: string
  account: Account | null
  positions: Position[]
  lastPrice: number | null
  paper: boolean
  onSubmitted: (order: Order) => void
}

const ORDER_TYPES = [
  { value: 'market', label: 'Market' },
  { value: 'limit', label: 'Limit' },
  { value: 'stop', label: 'Stop' },
  { value: 'stop_limit', label: 'Stop limit' },
]

const TIME_IN_FORCE = [
  { value: 'day', label: 'Day' },
  { value: 'gtc', label: 'GTC' },
  { value: 'ioc', label: 'IOC' },
  { value: 'fok', label: 'FOK' },
]

export function OrderTicket({
  symbol,
  account,
  positions,
  lastPrice,
  paper,
  onSubmitted,
}: OrderTicketProps) {
  const [side, setSide] = useState<'buy' | 'sell'>('buy')
  const [sizeMode, setSizeMode] = useState<SizeMode>('qty')
  const [orderType, setOrderType] = useState<OrderType>('market')
  const [qty, setQty] = useState<number | null>(1)
  const [notional, setNotional] = useState<number | null>(100)
  const [limitPrice, setLimitPrice] = useState<number | null>(null)
  const [stopPrice, setStopPrice] = useState<number | null>(null)
  const [timeInForce, setTimeInForce] = useState<TimeInForce>('day')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  const needsLimit = orderType === 'limit' || orderType === 'stop_limit'
  const needsStop = orderType === 'stop' || orderType === 'stop_limit'

  // A market order fills near the last trade; a limit/stop order uses its own
  // trigger price. Do not fall back to the last trade while that field is empty.
  const referencePrice = needsLimit ? limitPrice : needsStop ? stopPrice : lastPrice
  const estimate =
    sizeMode === 'qty'
      ? (qty ?? 0) * (referencePrice ?? 0)
      : (notional ?? 0)

  const buyingPower = num(account?.buying_power)
  const held = positions.find((position) => position.symbol === symbol)
  const heldQty = num(held?.qty) ?? num(held?.quantity)

  async function submit() {
    setError('')

    const request: CreateOrderRequest = {
      symbol,
      side,
      type: orderType,
      timeInForce,
    }

    if (sizeMode === 'qty') {
      if (!qty || qty <= 0) {
        setError('Enter a quantity.')
        return
      }
      request.qty = qty
    } else {
      if (!notional || notional <= 0) {
        setError('Enter a notional.')
        return
      }
      request.notional = notional
    }

    if (needsLimit) {
      if (!limitPrice || limitPrice <= 0) {
        setError('Enter a limit price.')
        return
      }
      request.limitPrice = limitPrice
    }

    if (needsStop) {
      if (!stopPrice || stopPrice <= 0) {
        setError('Enter a stop price.')
        return
      }
      request.stopPrice = stopPrice
    }

    setBusy(true)
    try {
      const order = await api.createOrder(request)
      onSubmitted(order)
    } catch (err) {
      setError(apiError(err))
    } finally {
      setBusy(false)
    }
  }

  const buttonLabel = `${side === 'buy' ? 'Buy' : 'Sell'} ${symbol}`

  return (
    <div className="ticket">
      <div className="ticket__heading">
        <span>Order ticket</span>
        <span className="ticket__symbol">{symbol}</span>
      </div>

      <SegmentedControl
        value={side}
        onChange={(value) => setSide(value === 'sell' ? 'sell' : 'buy')}
        label="Order side"
        layout="fill"
      >
        <SegmentedControlItem value="buy" label="Buy" />
        <SegmentedControlItem value="sell" label="Sell" />
      </SegmentedControl>

      <SegmentedControl
        value={sizeMode}
        onChange={(value) => setSizeMode(value === 'notional' ? 'notional' : 'qty')}
        label="Size in"
        size="sm"
        layout="fill"
      >
        <SegmentedControlItem value="qty" label="Shares" />
        <SegmentedControlItem value="notional" label="Notional" />
      </SegmentedControl>

      {sizeMode === 'qty' ? (
        <NumberInput
          label="Quantity"
          value={qty}
          onChange={setQty}
          min={0}
          step={1}
          isDisabled={busy}
        />
      ) : (
        <NumberInput
          label="Notional"
          value={notional}
          onChange={setNotional}
          min={0}
          step={10}
          isDisabled={busy}
          units="$"
        />
      )}

      <div className="ticket__row">
        <Selector
          label="Type"
          options={ORDER_TYPES}
          value={orderType}
          onChange={(value) => setOrderType(value as OrderType)}
          isDisabled={busy}
        />
        <Selector
          label="TIF"
          options={TIME_IN_FORCE}
          value={timeInForce}
          onChange={(value) => setTimeInForce(value as TimeInForce)}
          isDisabled={busy}
        />
      </div>

      {needsLimit || needsStop ? (
        <div className="ticket__row">
          {needsLimit ? (
            <NumberInput
              label="Limit"
              value={limitPrice}
              onChange={setLimitPrice}
              min={0}
              step={0.01}
              placeholder="0.00"
              isDisabled={busy}
              units="$"
            />
          ) : null}
          {needsStop ? (
            <NumberInput
              label="Stop"
              value={stopPrice}
              onChange={setStopPrice}
              min={0}
              step={0.01}
              placeholder="0.00"
              isDisabled={busy}
              units="$"
            />
          ) : null}
        </div>
      ) : null}

      <div className="ticket__summary">
        <div className="ticket__summary-row">
          <span>{side === 'buy' ? 'Est. cost' : 'Est. proceeds'}</span>
          <span>{estimate > 0 ? fmtMoney(estimate) : '—'}</span>
        </div>
        <div className="ticket__summary-row">
          <span>Buying power</span>
          <span>{buyingPower === null ? '—' : fmtMoney(buyingPower)}</span>
        </div>
        {side === 'sell' && heldQty ? (
          <div className="ticket__summary-row">
            <span>Held</span>
            <span>{heldQty} shares</span>
          </div>
        ) : null}
      </div>

      <Button
        label={buttonLabel}
        variant={side === 'buy' ? 'primary' : 'destructive'}
        onClick={submit}
        isLoading={busy}
        isDisabled={busy || !symbol}
        width="100%"
      />

      {error ? <div className="ticket__error">{error}</div> : null}

      <div className="ticket__hint">
        {paper ? 'Paper' : 'Live'} trading · keys are never saved.
      </div>
    </div>
  )
}
