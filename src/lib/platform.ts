import { invoke } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'

export type Platform = 'macos' | 'windows' | 'linux'

function detectPlatformSync(): Platform {
  if (typeof navigator === 'undefined') return 'windows'
  const ua = navigator.userAgent.toLowerCase()
  const plat = (navigator.platform || '').toLowerCase()
  if (plat.includes('mac') || ua.includes('macintosh') || ua.includes('mac os')) {
    return 'macos'
  }
  if (plat.includes('linux') || ua.includes('linux') || ua.includes('x11')) {
    return 'linux'
  }
  return 'windows'
}

let cachedPlatform: Platform = detectPlatformSync()

/**
 * Initializes and syncs platform with Tauri backend if available.
 */
export async function initPlatform(): Promise<Platform> {
  try {
    const os = await invoke<string>('get_platform')
    if (os === 'macos' || os === 'windows' || os === 'linux') {
      cachedPlatform = os
    }
  } catch {
    // Fall back to navigator-detected platform
  }
  return cachedPlatform
}

export function getPlatform(): Platform {
  return cachedPlatform
}

export function isMac(): boolean {
  return cachedPlatform === 'macos'
}

/**
 * Returns the OS-appropriate modifier key text.
 * macOS: '⌘'
 * Windows / Linux: 'Ctrl'
 */
export function getModKey(): string {
  return isMac() ? '⌘' : 'Ctrl'
}

/**
 * Returns formatted shortcut label with proper separator.
 * macOS: '⌘K'
 * Windows / Linux: 'Ctrl+K'
 */
export function getShortcutLabel(key: string): string {
  return isMac() ? `⌘${key.toUpperCase()}` : `Ctrl+${key.toUpperCase()}`
}

/**
 * React hook returning current platform and OS-aware shortcut label.
 */
export function usePlatformShortcut(key: string): string {
  const [label, setLabel] = useState(() => getShortcutLabel(key))

  useEffect(() => {
    void initPlatform().then(() => {
      setLabel(getShortcutLabel(key))
    })
  }, [key])

  return label
}
