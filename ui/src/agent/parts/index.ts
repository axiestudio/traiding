/**
 * Barrel for the assistant-ui message parts.
 *
 * A message part is chosen by its `type`, so this map is the single place that
 * decides how streamed content renders.
 */

export { MarkdownPart } from './MarkdownPart'
export { TextPart } from './TextPart'
export { ToolCallPart } from './ToolCallPart'
