/**
 * Agent surface — public API of this directory.
 *
 * Everything outside imports from here, never from a file inside it. That keeps
 * the assistant-ui wiring contained in one folder and makes the boundary
 * obvious when the rest of the terminal changes.
 */

export { AgentPanel } from './AgentPanel'
export { getAgentStatus, agentChat, agentError } from './ipc'
export type { AgentStatus, StreamEvent, WireMessage, ChatRequest } from './ipc'
export { tradingAdapter } from './adapter'
