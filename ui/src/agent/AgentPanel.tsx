/**
 * The agent pane.
 *
 * Owns the runtime and the configured/unconfigured gate. When no key is
 * present the composer is replaced by an instruction, because a chat box that
 * cannot answer is worse than a short explanation.
 */

import { useEffect, useState } from 'react'
import {
  AssistantRuntimeProvider,
  ThreadPrimitive,
  useLocalRuntime,
} from '@assistant-ui/react'

import { tradingAdapter } from './adapter'
import { AgentComposer } from './AgentComposer'
import { AgentMessage } from './AgentMessage'
import { getAgentStatus, type AgentStatus } from './ipc'

const SUGGESTIONS = [
  'What am I holding?',
  'Quote AAPL',
  'Summarise my open orders',
]

function Thread() {
  return (
    <ThreadPrimitive.Root className="agent-thread">
      <ThreadPrimitive.Viewport className="agent-thread__viewport">
        <ThreadPrimitive.Empty>
          <div className="agent-empty">
            <p className="agent-empty__title">Trading assistant</p>
            <p className="agent-empty__hint">
              Ask about quotes, positions or orders. Answers use this account.
            </p>
            <div className="agent-empty__chips">
              {SUGGESTIONS.map((text) => (
                <ThreadPrimitive.Suggestion
                  key={text}
                  prompt={text}
                  method="replace"
                  autoSend
                  className="agent-chip"
                >
                  {text}
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
      <div className="agent-panel">
        <div className="agent-empty">
          <p className="agent-empty__title">No model key</p>
          <p className="agent-empty__hint">
Add OPENROUTER_API_KEY to ~/.config/dev.traiding.terminal/.env
          </p>
        </div>
      </div>
    )
  }

  return (
    <div className="agent-panel">
      <AssistantRuntimeProvider runtime={runtime}>
        <Thread />
      </AssistantRuntimeProvider>
    </div>
  )
}
