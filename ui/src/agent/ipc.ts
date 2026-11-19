/**
 * IPC wrappers for the BYOK agent.
 *
 * The OpenRouter key never reaches this file: `agentChat` streams provider
 * output through a Tauri channel, and Rust holds the credential.
 */

import { Channel, invoke } from '@tauri-apps/api/core'

/** What the UI may know about the agent's key. */
export interface AgentStatus {
  configured: boolean
  model: string
}

/** One message in the OpenRouter wire format. */
export interface WireMessage {
  role: 'system' | 'user' | 'assistant' | 'tool'
  content?: string
  tool_calls?: WireToolCall[]
  tool_call_id?: string
}

export interface WireToolCall {
  id: string
  type: 'function'
  function: { name: string; arguments: string }
}

/** Events pushed from Rust while a turn runs. */
export type StreamEvent =
  | { type: 'delta'; text: string }
  | { type: 'toolStart'; id: string; name: string; arguments: string }
  | { type: 'toolDone'; id: string; name: string; result: string; isError: boolean }
  | { type: 'done'; text: string; rounds: number }

export interface ChatRequest {
  messages: WireMessage[]
  model?: string
}

/** Read the agent's configuration. Never returns the key itself. */
export function getAgentStatus(): Promise<AgentStatus> {
  return invoke<AgentStatus>('agent_status')
}

/** Validate and save a new OpenRouter key. The key is never returned. */
export function setAgentKey(key: string): Promise<AgentStatus> {
  return invoke<AgentStatus>('agent_set_key', { key })
}

/**
 * Run one assistant turn.
 *
 * `onEvent` is called for every streamed chunk. The returned promise settles
 * when the turn is over, so callers can clear their busy state in `finally`.
 */
export function agentChat(
  request: ChatRequest,
  onEvent: (event: StreamEvent) => void,
): Promise<void> {
  const channel = new Channel<StreamEvent>()
  channel.onmessage = onEvent

  return invoke<void>('agent_chat', { req: request, onEvent: channel })
}

export interface AgentError {
  message: string
}

/** Providers reject with a plain string; normalise anything else. */
export function agentError(err: unknown): string {
  if (typeof err === 'string') return err
  if (err instanceof Error) return err.message
  if (err && typeof err === 'object' && 'message' in err) {
    const message = (err as AgentError).message
    if (typeof message === 'string') return message
  }
  return 'The agent request failed.'
}
