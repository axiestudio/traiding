/**
 * Markdown text part.
 *
 * Assistant replies arrive as markdown, so headings, lists and code fences
 * should render instead of showing their source. Links leave the terminal
 * through the opener plugin; letting an anchor navigate the webview would
 * replace the app with a web page and drop the session.
 */

import { MarkdownTextPrimitive } from '@assistant-ui/react-markdown'
import { openUrl } from '@tauri-apps/plugin-opener'

export function MarkdownPart() {
  return (
    <MarkdownTextPrimitive
      className="agent-text agent-md"
      smooth={false}
      components={{
        a: ({ href, children }) => (
          <a
            className="agent-md__link"
            href={href}
            onClick={(event) => {
              event.preventDefault()
              if (href) void openUrl(href).catch(() => {})
            }}
          >
            {children}
          </a>
        ),
      }}
    />
  )
}
