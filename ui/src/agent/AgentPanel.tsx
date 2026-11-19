/**
 * The agent pane.
 *
 * The pane owns the runtime and the key gate. No key means a form, not a dead
 * end; a configured account gets a compact one-line bar for changing it.
 */

import { useEffect, useState } from 'react'
import {
  AssistantRuntimeProvider,
  ThreadPrimitive,
  useLocalRuntime,
} from '@assistant-ui/react'

import { tradingAdapter } from './adapter'
import { AgentComposer } from './AgentComposer'
import { AgentKeyForm } from './AgentKeyForm'
import { AgentMessage } from './AgentMessage'
import { getAgentStatus, type AgentStatus } from './ipc'

const SUGGESTIONS = [
  { label: 'Positions', prompt: 'What am I holding?' },
  { label: 'Orders', prompt: 'Summarise my open orders' },
  { label: 'Quote', prompt: 'Quote AAPL' },
]

interface ThreadProps {
  editingKey: boolean
  onChangeKey: () => void
  onKeySaved: (status: AgentStatus) => void
  onKeyCancel: () => void
}

function Thread({ editingKey, onChangeKey, onKeySaved, onKeyCancel }: ThreadProps) {
  return (
    <ThreadPrimitive.Root className="agent-thread">
      <div className="agent-bar">
        <span className="agent-bar__provider">OpenRouter</span>
        {editingKey ? null : (
          <button type="button" className="agent-bar__key" onClick={onChangeKey}>
            Change key
          </button>
        )}
      </div>

      {editingKey ? (
        <div className="agent-key-wrap">
          <AgentKeyForm onSaved={onKeySaved} onCancel={onKeyCancel} />
        </div>
      ) : null}

      <ThreadPrimitive.Viewport className="agent-thread__viewport">
        <ThreadPrimitive.Empty>
          <div className="agent-empty">
            <div className="agent-empty__chips">
              {SUGGESTIONS.map(({ label, prompt }) => (
                <ThreadPrimitive.Suggestion
                  key={prompt}
                  prompt={prompt}
                  method="replace"
                  autoSend
                  className="agent-chip"
                >
                  {label}
                </ThreadPrimitive.Suggestion>
              ))}
            </div>
          </div>
        </ThreadPrimitive.Empty>

        <ThreadPrimitive.Messages components={{ Message: AgentMessage }} />
      </ThreadPrimitive.Viewport>

      <AgentComposer />
    </ThreadPrimitive.Root>
  )
}

export function AgentPanel() {
  const [status, setStatus] = useState<AgentStatus | null>(null)
  const [editingKey, setEditingKey] = useState(false)
  const runtime = useLocalRuntime(tradingAdapter)

  useEffect(() => {
    let cancelled = false

    getAgentStatus()
      .then((next) => {
        if (!cancelled) setStatus(next)
      })
      .catch(() => {
        if (!cancelled) setStatus({ configured: false, model: '' })
      })

    return () => {
      cancelled = true
    }
  }, [])

  if (status === null) {
    return <div className="agent-panel agent-panel--loading" />
  }

  if (!status.configured) {
    return (
      <div className="agent-panel agent-panel--setup">
        <AgentKeyForm onSaved={setStatus} />
      </div>
    )
  }

  return (
    <div className="agent-panel">
      <AssistantRuntimeProvider runtime={runtime}>
        <Thread
          editingKey={editingKey}
          onChangeKey={() => setEditingKey((open) => !open)}
          onKeySaved={(next) => {
            setStatus(next)
            setEditingKey(false)
          }}
          onKeyCancel={() => setEditingKey(false)}
        />
      </AssistantRuntimeProvider>
    </div>
  )
}
