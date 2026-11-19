/**
 * One message bubble.
 *
 * User turns read as prompts and assistant turns as output, so the two are
 * styled apart: same alignment, different weight and colour. Assistant turns
 * render markdown and tool calls inline, and surface the run error when one
 * happens — a failed turn must never look like a silent one.
 */

import { MessagePrimitive, useAuiState } from '@assistant-ui/react'

import { MarkdownPart, TextPart, ToolCallPart } from './parts'

const userParts = { Text: TextPart }
const assistantParts = { Text: MarkdownPart, tools: { Fallback: ToolCallPart } }

/** The provider's sentence, read from the error assistant-ui stored. */
function AgentErrorText() {
  const message = useAuiState((state) => {
    const status = state.message.status
    if (status?.type !== 'incomplete' || status.reason !== 'error') return ''

    const error = status.error
    if (typeof error === 'string') return error
    if (
      error &&
      typeof error === 'object' &&
      'message' in error &&
      typeof error.message === 'string'
    ) {
      return error.message
    }
    return 'The agent request failed.'
  })

  return <p className="agent-error">{message}</p>
}

export function AgentMessage() {
  return (
    <MessagePrimitive.Root className="agent-msg">
      <MessagePrimitive.If user>
        <div className="agent-msg__body agent-msg__body--user">
          <MessagePrimitive.Parts components={userParts} />
        </div>
      </MessagePrimitive.If>

      <MessagePrimitive.If assistant>
        <div className="agent-msg__body agent-msg__body--assistant">
          <MessagePrimitive.Parts components={assistantParts} />
          <MessagePrimitive.Error>
            <AgentErrorText />
          </MessagePrimitive.Error>
        </div>
      </MessagePrimitive.If>
    </MessagePrimitive.Root>
  )
}
