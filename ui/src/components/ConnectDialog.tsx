/**
 * The one gate between an empty window and a live terminal.
 *
 * Credentials are posted once to the Rust process, validated against
 * `/v2/account` there, and never returned. Only the masked key comes back, so
 * the webview cannot leak them even through a devtools inspection.
 */

import { useState } from 'react'
import { Button } from '@astryxdesign/core/Button'
import { Dialog } from '@astryxdesign/core/Dialog'
import { DialogHeader } from '@astryxdesign/core/Dialog'
import { HStack } from '@astryxdesign/core/HStack'
import { Layout } from '@astryxdesign/core/Layout'
import { LayoutContent } from '@astryxdesign/core/Layout'
import { LayoutFooter } from '@astryxdesign/core/Layout'
import { SegmentedControl } from '@astryxdesign/core/SegmentedControl'
import { SegmentedControlItem } from '@astryxdesign/core/SegmentedControl'
import { TextInput } from '@astryxdesign/core/TextInput'
import { VStack } from '@astryxdesign/core/VStack'
import { openUrl } from '@tauri-apps/plugin-opener'

import { api, apiError } from '../api'
import type { SessionInfo } from '../api'

interface ConnectDialogProps {
  isOpen: boolean
  onConnected: (session: SessionInfo) => void
}

const DASHBOARD_URL = 'https://app.alpaca.markets/paper/dashboard/overview'

/**
 * Alpaca issues paper keys with a `PK` prefix and live keys with `AK`, and each
 * is only valid on its own host. Returns the mode implied by the prefix, or
 * `null` when the input does not follow the convention.
 */
function modeFromKeyId(keyId: string): 'paper' | 'live' | null {
  const key = keyId.trim()
  if (key.startsWith('PK')) return 'paper'
  if (key.startsWith('AK')) return 'live'
  return null
}

export function ConnectDialog({ isOpen, onConnected }: ConnectDialogProps) {
  const [keyId, setKeyId] = useState('')
  const [secret, setSecret] = useState('')
  const [mode, setMode] = useState<'paper' | 'live'>('paper')
  // Once the user picks a mode by hand, stop overriding it from the key prefix.
  const [modePinned, setModePinned] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  const implied = modeFromKeyId(keyId)
  const modeError =
    implied !== null && implied !== mode
      ? `${implied === 'paper' ? 'Paper' : 'Live'} key in ${mode} mode. Switch mode.`
      : ''

  function updateKeyId(value: string) {
    setKeyId(value)
    const next = modeFromKeyId(value)
    if (next && !modePinned) setMode(next)
  }

  async function submit() {
    if (!keyId.trim() || !secret.trim()) {
      setError('Key ID and secret required.')
      return
    }

    if (modeError) {
      setError('')
      return
    }

    setBusy(true)
    setError('')
    try {
      const session = await api.connect(keyId.trim(), secret.trim(), mode)
      setSecret('')
      onConnected(session)
    } catch (err) {
      setError(apiError(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog
      isOpen={isOpen}
      // The terminal has no usable state without a session, so the dialog is
      // deliberately not dismissable.
      onOpenChange={() => {}}
      purpose="required"
      width={440}
    >
      <Layout
        header={<DialogHeader title="Connect to Alpaca" />}
        content={
          <LayoutContent>
            <VStack gap={3}>
              <p className="ticket__hint">
                Get keys from your{' '}
                <a
                  href={DASHBOARD_URL}
                  onClick={(event) => {
                    event.preventDefault()
                    void openUrl(DASHBOARD_URL)
                  }}
                >
                  Alpaca dashboard
                </a>
                .
              </p>

              <SegmentedControl
                value={mode}
                onChange={(value) => {
                  setModePinned(true)
                  setMode(value === 'live' ? 'live' : 'paper')
                }}
                label="Trading mode"
                layout="fill"
              >
                <SegmentedControlItem value="paper" label="Paper" />
                <SegmentedControlItem value="live" label="Live" />
              </SegmentedControl>

              {mode === 'live' ? (
                <p className="ticket__error">Live trading. Real money.</p>
              ) : null}

              <TextInput
                label="Key ID"
                description="PK is paper, AK is live."
                placeholder="PK3976TZ…"
                value={keyId}
                onChange={updateKeyId}
                hasAutoFocus
                isDisabled={busy}
              />

              {modeError ? <p className="ticket__error">{modeError}</p> : null}

              <TextInput
                label="Secret"
                description="Shown once when created."
                placeholder="••••••••••••••••"
                type="password"
                value={secret}
                onChange={setSecret}
                isDisabled={busy}
                onEnter={submit}
              />

              <p className="ticket__hint">
                Lost it? <b>Regenerate</b> in Alpaca.
              </p>

              {error ? <p className="ticket__error">{error}</p> : null}
            </VStack>
          </LayoutContent>
        }
        footer={
          <LayoutFooter hasDivider>
            <HStack gap={2} justify="end" align="center">
              <span className="dialog__security">Stored locally.</span>
              <Button
                label={busy ? 'Connecting…' : 'Connect'}
                variant="primary"
                onClick={submit}
                isLoading={busy}
                isDisabled={busy}
              />
            </HStack>
          </LayoutFooter>
        }
      />
    </Dialog>
  )
}
