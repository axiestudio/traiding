/**
 * The chat model adapter.
 *
 * assistant-ui owns the message list and composer; this adapter is the single
 * place that turns a thread into an IPC call. Rust does the streaming and the
 * tool execution, so nothing here knows about OpenRouter or Alpaca.
 */

import type { ChatModelAdapter, ChatModelRunResult } from '@assistant-ui/react'
import type { ReadonlyJSONObject } from 'assistant-stream/utils'

import { agentChat, agentError, type StreamEvent, type WireMessage } from './ipc'

type ToolState = {
  name: string
  args: string
  result?: string
  isError?: boolean
}

/** Bridge the push-based IPC callback to the adapter's async iteration. */
function createEventQueue() {
  const events: StreamEvent[] = []
  let wake: (() => void) | null = null
  let closed = false

  return {
    push(event: StreamEvent) {
      events.push(event)
      wake?.()
      wake = null
    },
    close() {
      closed = true
      wake?.()
      wake = null
    },
    /** Resolve with the next event, or null once the stream is finished. */
    async next(): Promise<StreamEvent | null> {
      while (events.length === 0) {
        if (closed) return null
        await new Promise<void>((resolve) => {
          wake = resolve
        })
      }
      return events.shift() ?? null
    },
  }
}

function safeArgs(raw: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(raw)
    return parsed && typeof parsed === 'object' ? parsed : {}
  } catch {
    // Arguments stream in pieces, so a partial string is normal mid-flight.
    return {}
  }
}

type ChatContent = NonNullable<ChatModelRunResult['content']>

/** One rendered stretch, in the order the provider emitted it. */
type Segment = { kind: 'text'; text: string } | { kind: 'tool'; id: string }

function buildContent(
  segments: Segment[],
  tools: Map<string, ToolState>,
): ChatContent {
  const parts: ChatContent[number][] = []

  for (const segment of segments) {
    if (segment.kind === 'text') {
      if (segment.text) parts.push({ type: 'text', text: segment.text })
      continue
    }

    const tool = tools.get(segment.id)
    if (!tool) continue
    parts.push({
      type: 'tool-call',
      toolCallId: segment.id,
      toolName: tool.name,
      args: safeArgs(tool.args) as ReadonlyJSONObject,
      argsText: tool.args,
      result: tool.result,
      isError: tool.isError,
    })
  }

  return parts
}

/**
 * Flatten assistant-ui's thread into the provider's wire shape.
 *
 * Rust is stateless between turns, so the whole conversation is rebuilt here.
 */
function toWireMessages(
  messages: readonly { role: string; content: unknown }[],
): WireMessage[] {
  const wire: WireMessage[] = []

  for (const message of messages) {
    if (message.role !== 'user' && message.role !== 'assistant') continue
    if (!Array.isArray(message.content)) continue

    const text = message.content
      .filter((part): part is { type: 'text'; text: string } => part?.type === 'text')
      .map((part) => part.text)
      .join('')

    if (text.trim()) {
      wire.push({ role: message.role as 'user' | 'assistant', content: text })
    }
  }

  return wire
}

export const tradingAdapter: ChatModelAdapter = {
  async *run({ messages, abortSignal }) {
    const tools = new Map<string, ToolState>()
    const segments: Segment[] = []
    const queue = createEventQueue()

    const run = agentChat({ messages: toWireMessages(messages) }, (event) => {
      queue.push(event)
    })
      .catch((err) => {
        throw new Error(agentError(err))
      })
      .finally(() => queue.close())

    // Keep the unhandled rejection from surfacing before the loop awaits it.
    const settled = run.then(
      () => undefined,
      (err: unknown) => {
        throw err
      },
    )
    settled.catch(() => {})

    while (true) {
      const event = await queue.next()
      if (event === null) break

      if (abortSignal.aborted) break

      switch (event.type) {
        case 'delta': {
          // Append to the open text stretch so a reply stays one part until a
          // tool call interrupts it; then the next delta starts a new one.
          const last = segments[segments.length - 1]
          if (last?.kind === 'text') {
            last.text += event.text
          } else {
            segments.push({ kind: 'text', text: event.text })
          }
          break
        }
        case 'toolStart':
          tools.set(event.id, { name: event.name, args: event.arguments })
          segments.push({ kind: 'tool', id: event.id })
          break
        case 'toolDone': {
          const existing: ToolState = tools.get(event.id) ?? {
            name: event.name,
            args: '{}',
          }
          existing.result = event.result
          existing.isError = event.isError
          tools.set(event.id, existing)
          break
        }
        case 'done':
          // Deltas already carry the reply in order. Use the final snapshot
          // only when nothing streamed, as a one-shot provider would send.
          if (event.text && !segments.some((segment) => segment.kind === 'text')) {
            segments.push({ kind: 'text', text: event.text })
          }
          break
      }

      yield { content: buildContent(segments, tools) }
    }

    await settled
    yield { content: buildContent(segments, tools) }
  },
}
