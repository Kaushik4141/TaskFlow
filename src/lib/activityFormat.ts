/**
 * Utility functions for sanitizing, formatting, and humanizing
 * activity titles, window titles, and application identifiers.
 */

// Known app ID / process name mappings to clean human-readable names
const KNOWN_APPS: Record<string, string> = {
  'com.mitchellh.ghostty': 'Ghostty',
  'ghostty': 'Ghostty',
  'code-oss': 'VS Code',
  'code': 'VS Code',
  'vscode': 'VS Code',
  'cursor': 'Cursor',
  'org.kde.dolphin': 'Dolphin',
  'dolphin': 'Dolphin',
  'org.mozilla.firefox': 'Firefox',
  'firefox': 'Firefox',
  'google-chrome': 'Chrome',
  'chrome': 'Chrome',
  'chromium': 'Chromium',
  'helium': 'Helium',
  'zen': 'Zen Browser',
  'zen-browser': 'Zen Browser',
  'vibe-typer': 'Vibe Typer',
  'alacritty': 'Alacritty',
  'kitty': 'Kitty',
  'foot': 'Foot',
  'wezterm': 'WezTerm',
  'wezterm-gui': 'WezTerm',
  'org.gnome.terminal': 'Terminal',
  'terminal': 'Terminal',
  'sublime_text': 'Sublime Text',
  'slack': 'Slack',
  'discord': 'Discord',
  'spotify': 'Spotify',
  'obsidian': 'Obsidian',
  'notion': 'Notion',
  'figma': 'Figma',
}

/**
 * Normalizes an executable or package app identifier into a clean, human-readable name.
 * e.g. "com.mitchellh.ghostty" -> "Ghostty"
 *      "code-oss" -> "VS Code"
 *      "org.kde.dolphin" -> "Dolphin"
 *      "explorer.exe" -> "Explorer"
 */
export function cleanAppName(rawApp?: string | null): string {
  if (!rawApp || !rawApp.trim()) return 'App'
  const trimmed = rawApp.trim()

  // Case-insensitive exact match in known dictionary
  const lower = trimmed.toLowerCase()
  if (KNOWN_APPS[lower]) return KNOWN_APPS[lower]

  // Strip .exe, .app, and " Helper" suffixes
  let stripped = trimmed.replace(/\.(exe|app)$/i, '').replace(/ Helper$/i, '')
  const lowerStripped = stripped.toLowerCase()
  if (KNOWN_APPS[lowerStripped]) return KNOWN_APPS[lowerStripped]

  // Reverse-DNS normalization: e.g. "com.example.FooBar" or "org.kde.dolphin" -> "Dolphin"
  if (/^[a-z0-9_-]+(\.[a-z0-9_-]+)+$/i.test(stripped)) {
    const parts = stripped.split('.')
    const last = parts[parts.length - 1]
    if (last && last.length > 1) {
      const lowerLast = last.toLowerCase()
      if (KNOWN_APPS[lowerLast]) return KNOWN_APPS[lowerLast]
      // Capitalize first letter
      return last.charAt(0).toUpperCase() + last.slice(1)
    }
  }

  // Handle hyphenated or underscored names like "vibe-typer" -> "Vibe Typer"
  if (stripped.includes('-') || stripped.includes('_')) {
    return stripped
      .split(/[-_]/)
      .filter(Boolean)
      .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
      .join(' ')
  }

  // If already capitalized or camelCase, preserve or capitalize first letter
  return stripped.charAt(0).toUpperCase() + stripped.slice(1)
}

/**
 * Strips noise, notification counts, browser profile stamps,
 * and editor cruft from raw window titles.
 */
export function cleanWindowTitle(rawTitle?: string | null): string {
  if (!rawTitle || !rawTitle.trim()) return ''
  let t = rawTitle.trim()

  // 1. Strip notification badges: "(73) WhatsApp" -> "WhatsApp", "(1) GitHub" -> "GitHub"
  t = t.replace(/^\(\d+\+?\)\s*/, '')

  // 2. Strip editor dirty indicators and git worktree annotations:
  // "● linux_reader.rs" -> "linux_reader.rs"
  t = t.replace(/^[●*•]\s*/, '')
  // "linux_reader.rs (Working Tree)" -> "linux_reader.rs"
  t = t.replace(/\s*\(Working Tree\)/i, '')

  // 3. Strip browser trailing profile / brand suffixes:
  // " — Original profile — Mozilla Firefox"
  // " - Google Chrome"
  // " — Mozilla Firefox"
  // " - Helium"
  // " | Microsoft Teams"
  t = t.replace(
    /\s*[-—|•]\s*(?:Original profile\s*[-—|•]\s*)?(?:Mozilla Firefox|Google Chrome|Chromium|Brave|Microsoft Edge|Helium|Safari|Arc|Vivaldi|Zen Browser|Zen).*$/i,
    ''
  )

  // 4. Strip editor suffixes:
  // " - Visual Studio Code", " - Code - OSS", " - Cursor"
  t = t.replace(/\s*[-—|•]\s*(?:Visual Studio Code|VS Code|Code - OSS|Cursor|Sublime Text|VSCodium).*$/i, '')

  // 5. Strip terminal trailing shell indicators:
  // "hermes setup ~" -> "hermes setup"
  // "build — zsh" -> "build"
  t = t.replace(/\s*[-—|•]\s*(?:bash|zsh|fish|sh|terminal|ghostty|kitty)$/i, '')
  t = t.replace(/\s+~$/, '')

  // 6. Clean dangling separators and extra spaces
  t = t.replace(/\s*[-—|•]\s*$/, '').trim()

  return t
}

/**
 * Formats an activity rollup title into a clean, human-readable, professional heading.
 * Handles:
 * - Reverse-DNS app prefixes ("com.mitchellh.ghostty — prep" -> "Ghostty — prep")
 * - Browser garbage ("com.mitchellh.ghostty — (73) WhatsApp — Original profile —…" -> "WhatsApp")
 * - Action phrases ("Worked on: a, b, c,…" -> "Worked on a, b, c…")
 * - Redundant app mentions ("firefox — TaskFlow — Dolphin" -> "TaskFlow — Dolphin")
 */
export function formatActivityTitle(rawTitle?: string | null): string {
  if (!rawTitle || !rawTitle.trim()) return 'Activity'
  let title = rawTitle.trim()

  // Handle action-based titles: "Worked on: ...", "Ran commands: ...", "Reviewed: ..."
  const actionMatch = title.match(/^(Worked on|Ran commands|Reviewed|Created|Updated|Debugged):\s*(.*)$/i)
  if (actionMatch) {
    const action = actionMatch[1]
    let rest = actionMatch[2].trim()
    // Clean trailing broken ellipses like ",…" or ", …"
    rest = rest.replace(/,\s*[…\.]+$/, '…')
    // Ensure clean ellipsis formatting
    rest = rest.replace(/\.{3,}$/, '…')
    return `${action} ${rest}`
  }

  // Check for app separator: "App — Window/Detail" or "App - Window/Detail"
  const sepMatch = title.match(/^(.*?)\s+[-—]\s+(.*)$/)
  if (sepMatch) {
    const rawApp = sepMatch[1].trim()
    const rawDetail = sepMatch[2].trim()

    const app = cleanAppName(rawApp)
    const detail = cleanWindowTitle(rawDetail)

    if (!detail) {
      return `${app} Activity`
    }

    // If detail already includes the app name (e.g. "TaskFlow — Dolphin"), just return the detail
    if (detail.toLowerCase().includes(app.toLowerCase())) {
      return detail
    }

    // Common standalone web apps / tools where the host browser/terminal isn't the primary subject
    // (e.g. if terminal ran WhatsApp or browser had WhatsApp tab)
    const STANDALONE_SERVICES = [
      'whatsapp',
      'telegram',
      'slack',
      'discord',
      'notion',
      'linear',
      'jira',
      'github',
      'gitlab',
      'composio',
      'claude',
      'chatgpt',
      'figma',
      'meet',
      'zoom',
    ]

    const detailLower = detail.toLowerCase()
    const isStandalone = STANDALONE_SERVICES.some((svc) => detailLower === svc || detailLower.startsWith(`${svc} `))
    if (isStandalone) {
      return detail
    }

    return `${app} — ${detail}`
  }

  // Standalone window title or single token
  const cleaned = cleanWindowTitle(title)
  return cleaned || cleanAppName(title)
}
