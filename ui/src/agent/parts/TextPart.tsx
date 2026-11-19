/**
 * Streaming text part.
 *
 * assistant-ui splits a message into parts; this renders the text ones and
 * keeps whitespace significant so markdown-ish output stays readable.
 */

import { MessagePartPrimitive } from '@assistant-ui/react'

export function TextPart() {
  return (
    <span className="agent-text">
      <MessagePartPrimitive.Text />
    </span>
  )
}
