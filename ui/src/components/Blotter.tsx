/**
 * The blotter: two tabs over the same lower-left pane, with live row counts.
 */

import { Tab } from '@astryxdesign/core/TabList'
import { TabList } from '@astryxdesign/core/TabList'

import type { Order, Position } from '../api'
import { OrdersTable } from './OrdersTable'
import { PositionsTable } from './PositionsTable'

export type BlotterTab = 'positions' | 'orders'

interface BlotterProps {
  tab: BlotterTab
  onTabChange: (tab: BlotterTab) => void
  positions: Position[]
  orders: Order[]
  onClosePosition: (symbol: string) => void
  onCancelOrder: (id: string) => void
  closingSymbol: string | null
  cancelingId: string | null
}

export function Blotter({
  tab,
  onTabChange,
  positions,
  orders,
  onClosePosition,
  onCancelOrder,
  closingSymbol,
  cancelingId,
}: BlotterProps) {
  return (
    <div className="blotter">
      <div className="blotter__tabs">
        <TabList
          value={tab}
          onChange={(value) => onTabChange(value as BlotterTab)}
          size="sm"
          hasDivider
        >
          <Tab
            value="positions"
            label={`Positions${
              positions.length ? ` (${positions.length})` : ''
            }`}
          />
          <Tab value="orders" label={`Orders${orders.length ? ` (${orders.length})` : ''}`} />
        </TabList>
      </div>

      <div className="blotter__body">
        {tab === 'positions' ? (
          <PositionsTable
            positions={positions}
            onClose={onClosePosition}
            closing={closingSymbol}
          />
        ) : (
          <OrdersTable orders={orders} onCancel={onCancelOrder} canceling={cancelingId} />
        )}
      </div>
    </div>
  )
}
