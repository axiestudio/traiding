/**
 * Top bar: mode badge, market clock, and the two numbers that matter at a
 * glance — equity and buying power.
 */

import { Badge } from '@astryxdesign/core/Badge'
import { Button } from '@astryxdesign/core/Button'
import { HStack } from '@astryxdesign/core/HStack'
import { StatusDot } from '@astryxdesign/core/StatusDot'

import { fmtMoney, num } from '../api'
import type { Account, Clock, SessionInfo } from '../api'

interface TopBarProps {
  session: SessionInfo
  account: Account | null
  clock: Clock | null
  feedLive: boolean
  onDisconnect: () => void
}

export function TopBar({ session, account, clock, feedLive, onDisconnect }: TopBarProps) {
  const isOpen = clock?.is_open === true
  const hasClock = clock !== null

  return (
    <header className="topbar">
      <div className="topbar__brand">
        traiding<span>.</span>
      </div>

      <Badge
        variant={session.paper ? 'warning' : 'error'}
        label={session.paper ? 'paper' : 'live'}
      />

      <div className="topbar__status">
        <StatusDot
          variant={!hasClock ? 'neutral' : isOpen ? 'success' : 'error'}
          label="market status"
          isPulsing={isOpen}
        />
        {hasClock ? (isOpen ? 'market open' : 'market closed') : 'market unknown'}
      </div>

      <div className="topbar__feed">
        <StatusDot
          variant={feedLive ? 'success' : 'neutral'}
          label={feedLive ? 'feed live' : 'feed polling'}
          isPulsing={feedLive}
        />
        <span>{feedLive ? 'feed live' : 'feed polling'}</span>
      </div>

      <div className="topbar__spacer" />

      <HStack gap={5} align="center">
        <div className="topbar__metric">
          <div>Equity</div>
          <div>{account ? fmtMoney(num(account.equity)) : '—'}</div>
        </div>

        <div className="topbar__metric">
          <div>Buying power</div>
          <div>{account ? fmtMoney(num(account.buying_power)) : '—'}</div>
        </div>

        {session.connected ? (
          <Button label="Disconnect" size="sm" variant="secondary" onClick={onDisconnect} />
        ) : null}
      </HStack>
    </header>
  )
}
