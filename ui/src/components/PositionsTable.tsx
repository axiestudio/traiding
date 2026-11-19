/**
 * Positions table: what the account currently holds, with unrealised P/L and a
 * one-click liquidate action.
 */

import { Button } from '@astryxdesign/core/Button'
import { Table, type TableColumn } from '@astryxdesign/core/Table'

import { fmtMoney, fmtQty, num, signed } from '../api'
import type { Position } from '../api'

interface PositionsTableProps {
  positions: Position[]
  onClose: (symbol: string) => void
  closing: string | null
}

export function PositionsTable({ positions, onClose, closing }: PositionsTableProps) {
  const columns: TableColumn<Position>[] = [
    {
      key: 'symbol',
      header: 'Symbol',
      width: { type: 'pixel', value: 96 },
      renderCell: (row) => <span className="cell-symbol">{row.symbol}</span>,
    },
    {
      key: 'qty',
      header: 'Qty',
      align: 'end',
      width: { type: 'pixel', value: 84 },
      renderCell: (row) => (
        <span className="cell-num">{fmtQty(num(row.qty) ?? num(row.quantity))}</span>
      ),
    },
    {
      key: 'avg',
      header: 'Avg entry',
      align: 'end',
      width: { type: 'pixel', value: 104 },
      renderCell: (row) => (
        <span className="cell-num">
          {fmtMoney(num(row.avg_entry_price) ?? num(row.average_entry_price))}
        </span>
      ),
    },
    {
      key: 'last',
      header: 'Last',
      align: 'end',
      width: { type: 'pixel', value: 96 },
      renderCell: (row) => <span className="cell-num">{fmtMoney(row.current_price)}</span>,
    },
    {
      key: 'value',
      header: 'Value',
      align: 'end',
      width: { type: 'pixel', value: 116 },
      renderCell: (row) => <span className="cell-num">{fmtMoney(row.market_value)}</span>,
    },
    {
      key: 'pl',
      header: 'P/L',
      align: 'end',
      width: { type: 'pixel', value: 132 },
      renderCell: (row) => {
        const pl = num(row.unrealized_pl) ?? num(row.unrealized_gain_total)
        const pct = num(row.unrealized_plpc) ?? num(row.unrealized_gain_total_percent)
        if (pl === null) return <span className="cell-num flat">—</span>
        return (
          <div className={pl > 0 ? 'up' : pl < 0 ? 'down' : 'flat'}>
            <div className="cell-num">{fmtMoney(pl)}</div>
            {pct !== null ? (
              <div className="cell-sub">{signed(pct * 100)}%</div>
            ) : null}
          </div>
        )
      },
    },
    {
      key: 'actions',
      header: '',
      align: 'end',
      width: { type: 'pixel', value: 88 },
      renderCell: (row) => (
        <Button
          label="Close"
          size="sm"
          variant="destructive"
          onClick={() => onClose(row.symbol)}
          isDisabled={closing !== null}
          isLoading={closing === row.symbol}
        />
      ),
    },
  ]

  if (positions.length === 0) {
    return <div className="blotter__empty">No open positions</div>
  }

  return (
    <Table
      data={positions}
      columns={columns}
      idKey="symbol"
      density="compact"
      dividers="rows"
      hasHover
      textOverflow="truncate"
    />
  )
}
