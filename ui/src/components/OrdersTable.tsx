/**
 * Orders blotter: everything Alpaca has on file, with cancel for anything still
 * working. Statuses are mapped to badge colours once, here.
 */

import { Badge } from '@astryxdesign/core/Badge'
import type { BadgeVariant } from '@astryxdesign/core/Badge'
import { Button } from '@astryxdesign/core/Button'
import { Table, type TableColumn } from '@astryxdesign/core/Table'

import { fmtMoney, fmtQty, num, sentenceCase } from '../api'
import type { Order } from '../api'

interface OrdersTableProps {
  orders: Order[]
  onCancel: (id: string) => void
  canceling: string | null
}

/** Statuses that can still be cancelled upstream. */
const OPEN_STATUSES = new Set([
  'new',
  'accepted',
  'pending_new',
  'partially_filled',
  'pending_replace',
  'held',
  'pending_cancel',
])

function statusVariant(status: string): BadgeVariant {
  switch (status) {
    case 'filled':
      return 'info'
    case 'new':
    case 'accepted':
    case 'pending_new':
    case 'partially_filled':
      return 'warning'
    case 'canceled':
    case 'expired':
    case 'rejected':
      return 'neutral'
    default:
      return 'neutral'
  }
}

function sideVariant(side: string | undefined): BadgeVariant {
  return side?.toLowerCase() === 'sell' ? 'error' : 'success'
}

export function OrdersTable({ orders, onCancel, canceling }: OrdersTableProps) {
  const columns: TableColumn<Order>[] = [
    {
      key: 'symbol',
      header: 'Symbol',
      width: { type: 'pixel', value: 96 },
      renderCell: (row) => <span className="cell-symbol">{row.symbol}</span>,
    },
    {
      key: 'side',
      header: 'Side',
      width: { type: 'pixel', value: 76 },
      renderCell: (row) => (
        <Badge
          variant={sideVariant(row.side)}
          label={row.side ? sentenceCase(row.side) : '—'}
        />
      ),
    },
    {
      key: 'type',
      header: 'Type',
      width: { type: 'pixel', value: 96 },
      renderCell: (row) => {
        const raw = row.order_type ?? row.type ?? ''
        return <span className="cell-num">{raw ? sentenceCase(raw) : '—'}</span>
      },
    },
    {
      key: 'qty',
      header: 'Qty',
      align: 'end',
      width: { type: 'pixel', value: 80 },
      renderCell: (row) => <span className="cell-num">{fmtQty(row.qty)}</span>,
    },
    {
      key: 'filled',
      header: 'Filled',
      align: 'end',
      width: { type: 'pixel', value: 80 },
      renderCell: (row) => <span className="cell-num">{fmtQty(row.filled_qty)}</span>,
    },
    {
      key: 'avg',
      header: 'Avg price',
      align: 'end',
      width: { type: 'pixel', value: 104 },
      renderCell: (row) => {
        const filled = num(row.filled_avg_price)
        return (
          <span className={`cell-num ${filled === null ? 'flat' : ''}`}>
            {filled === null ? '—' : fmtMoney(filled)}
          </span>
        )
      },
    },
    {
      key: 'status',
      header: 'Status',
      width: { type: 'pixel', value: 112 },
      renderCell: (row) => {
        const status = row.status ?? 'unknown'
        return <Badge variant={statusVariant(status)} label={sentenceCase(status)} />
      },
    },
    {
      key: 'actions',
      header: '',
      align: 'end',
      width: { type: 'pixel', value: 84 },
      renderCell: (row) => {
        if (!row.status || !OPEN_STATUSES.has(row.status)) return null
        return (
          <Button
            label="Cancel"
            size="sm"
            variant="secondary"
            onClick={() => onCancel(row.id)}
            isDisabled={canceling !== null}
            isLoading={canceling === row.id}
          />
        )
      },
    },
  ]

  if (orders.length === 0) {
    return <div className="blotter__empty">No orders</div>
  }

  return (
    <Table
      data={orders}
      columns={columns}
      idKey="id"
      density="compact"
      dividers="rows"
      hasHover
      textOverflow="truncate"
    />
  )
}
