/**
 * Composer.
 *
 * Enter sends, Shift+Enter adds a line — the convention every chat surface
 * uses. The input is a textarea so multi-line prompts stay usable.
 */

import { ComposerPrimitive } from '@assistant-ui/react'

export function AgentComposer() {
  return (
    <ComposerPrimitive.Root className="agent-composer">
      <ComposerPrimitive.Input
        className="agent-composer__input"
        placeholder="Ask the agent"
        aria-label="Message the agent"
        rows={2}
      />
      <ComposerPrimitive.Send className="agent-composer__send">
        Send
      </ComposerPrimitive.Send>
    </ComposerPrimitive.Root>
  )
}
