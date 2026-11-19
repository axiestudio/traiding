/**
 * Entry point.
 *
 * Theme wraps the whole tree so the window, native scrollbars and form controls
 * all follow the same colour scheme as the components.
 *
 * Vite alone (no Tauri shell) has no IPC backend, so a dev-only mock installs
 * itself first. Inside Tauri the real `__TAURI_INTERNALS__` is already present
 * and the mock is never loaded; the production bundle drops it entirely.
 */

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { Theme } from '@astryxdesign/core/theme'
import { neutralTheme } from '@astryxdesign/theme-neutral/built'

import { App } from './App'
import './styles.css'

const container = document.getElementById('root')
if (!container) throw new Error('missing #root')

async function start(): Promise<void> {
  if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) {
    try {
      const { installMockTauri } = await import('./dev/mockTauri')
      installMockTauri()
    } catch (error) {
      console.error('[traiding] dev IPC mock failed to load', error)
    }
  }

  createRoot(container as HTMLElement).render(
    <StrictMode>
      <Theme theme={neutralTheme} mode="dark">
        <App />
      </Theme>
    </StrictMode>,
  )
}

void start()
