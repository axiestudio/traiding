/**
 * Model key form.
 *
 * The key goes straight to Rust, is checked there, and is written only to the
 * app-config .env. It is never read back into this component.
 */

import { useState, type FormEvent } from 'react'
import { Button } from '@astryxdesign/core/Button'
import { TextInput } from '@astryxdesign/core/TextInput'

import { agentError, setAgentKey, type AgentStatus } from './ipc'

interface AgentKeyFormProps {
  onSaved: (status: AgentStatus) => void
  onCancel?: () => void
}

export function AgentKeyForm({ onSaved, onCancel }: AgentKeyFormProps) {
  const [key, setKey] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function save(event: FormEvent) {
    event.preventDefault()
    if (!key.trim() || busy) return

    setBusy(true)
    setError(null)
    try {
      const status = await setAgentKey(key.trim())
      setKey('')
      onSaved(status)
    } catch (err) {
      setError(agentError(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <form className="agent-key" onSubmit={save}>
      <TextInput
        type="password"
        label="OpenRouter key"
        placeholder="sk-or-…"
        value={key}
        onChange={setKey}
        isDisabled={busy}
        autoComplete="off"
        hasAutoFocus
      />
      <div className="agent-key__row">
        {error ? null : <span className="agent-key__hint">Stored locally.</span>}
        <div className="agent-key__actions">
          {onCancel ? (
            <Button label="Cancel" size="sm" variant="ghost" onClick={onCancel} />
          ) : null}
          <Button
            label="Save"
            size="sm"
            type="submit"
            isLoading={busy}
            isDisabled={!key.trim()}
          />
        </div>
      </div>
      {error ? (
        <p className="agent-key__error" role="alert">
          {error}
        </p>
      ) : null}
    </form>
  )
}
