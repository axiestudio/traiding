/**
 * Tool call part.
 *
 * The model's tool calls are shown, not hidden: a trading assistant that reads
 * live account data should be visibly doing so. Arguments and results stay
 * collapsed to one line each so a long history stays scannable.
 */

import type { ToolCallMessagePartComponent } from '@assistant-ui/react'

import { fmtJson, toolLabel } from '../format'

export const ToolCallPart: ToolCallMessagePartComponent = ({
  toolName,
  argsText,
  result,
  isError,
  status,
}) => {
  const running = result === undefined && status?.type !== 'complete'

  return (
    <details className={`agent-tool${isError ? ' agent-tool--error' : ''}`}>
      <summary className="agent-tool__summary">
        <span className="agent-tool__dot" data-running={running} />
        <span className="agent-tool__name">{toolLabel(toolName)}</span>
        <span className="agent-tool__args">{fmtJson(argsText)}</span>
      </summary>
      {result !== undefined && (
        <pre className="agent-tool__result">{fmtJson(result)}</pre>
      )}
    </details>
  )
}
