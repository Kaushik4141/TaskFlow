interface AppLogoProps {
  appName: string
  size?: 'sm' | 'md' | 'lg'
  className?: string
}

export default function AppLogo({ appName, size = 'sm', className = '' }: AppLogoProps) {
  const clean = appName.toLowerCase().replace(/\.exe$/, '').trim()

  const sizeClasses = {
    sm: 'h-4 w-4',
    md: 'h-6 w-6',
    lg: 'h-8 w-8',
  }[size]

  const svgSizes = {
    sm: 'h-2.5 w-2.5',
    md: 'h-3.5 w-3.5',
    lg: 'h-5 w-5',
  }[size]

  const fontSizes = {
    sm: 'text-[9px]',
    md: 'text-xs',
    lg: 'text-sm',
  }[size]

  // Google Chrome
  if (clean.includes('chrome')) {
    return (
      <span
        title="Google Chrome"
        className={`inline-flex shrink-0 items-center justify-center rounded-full bg-white shadow-sm overflow-hidden ${sizeClasses} ${className}`}
      >
        <svg viewBox="0 0 24 24" className={size === 'sm' ? 'h-3.5 w-3.5' : size === 'md' ? 'h-5 w-5' : 'h-7 w-7'}>
          <circle cx="12" cy="12" r="11" fill="#EA4335" />
          <path d="M12 1a11 11 0 0 1 9.5 5.5L12 12V1z" fill="#EA4335" />
          <path d="M21.5 6.5A11 11 0 0 1 12 23l4.8-8.2 4.7-8.3z" fill="#FBBC05" />
          <path d="M12 23A11 11 0 0 1 2.5 6.5L12 12v11z" fill="#34A853" />
          <circle cx="12" cy="12" r="5" fill="#ffffff" />
          <circle cx="12" cy="12" r="4" fill="#4285F4" />
        </svg>
      </span>
    )
  }

  // VS Code / Cursor / IDE
  if (clean.includes('code') || clean.includes('cursor')) {
    return (
      <span
        title={appName}
        className={`inline-flex shrink-0 items-center justify-center rounded-full bg-[#007ACC] text-white shadow-sm ${sizeClasses} ${className}`}
      >
        <svg viewBox="0 0 24 24" fill="currentColor" className={svgSizes}>
          <path d="M23.15 2.587L18.21.21a1.494 1.494 0 0 0-1.705.29l-9.46 8.63-4.12-3.128a.999.999 0 0 0-1.276.057L.327 7.261A1 1 0 0 0 .32 8.653l3.65 3.344-3.65 3.345a1 1 0 0 0-.007 1.392l1.322 1.202a1 1 0 0 0 1.276.057l4.12-3.128 9.46 8.63a1.492 1.492 0 0 0 1.704.29l4.94-2.377A1.5 1.5 0 0 0 24 20.06V3.939a1.5 1.5 0 0 0-.85-1.352zM18 13.48l-5.63-4.14 5.63-4.14v8.28z" />
        </svg>
      </span>
    )
  }

  // Slack
  if (clean.includes('slack')) {
    return (
      <span
        title="Slack"
        className={`inline-flex shrink-0 items-center justify-center rounded-full bg-[#4A154B] text-white shadow-sm ${sizeClasses} ${className}`}
      >
        <svg viewBox="0 0 24 24" fill="currentColor" className={`${svgSizes} text-[#E01E5A]`}>
          <path d="M6 15a2 2 0 0 1-2 2 2 2 0 0 1-2-2 2 2 0 0 1 2-2h2v2zm1 0a2 2 0 0 1 2-2 2 2 0 0 1 2 2v5a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-5zm2-7a2 2 0 0 1-2-2 2 2 0 0 1 2-2 2 2 0 0 1 2 2v2H9zm0 1a2 2 0 0 1 2 2 2 2 0 0 1-2 2H4a2 2 0 0 1-2-2 2 2 0 0 1 2-2h5zm7 2a2 2 0 0 1 2-2 2 2 0 0 1 2 2 2 2 0 0 1-2 2h-2v-2zm-1 0a2 2 0 0 1-2 2 2 2 0 0 1-2-2V6a2 2 0 0 1 2-2 2 2 0 0 1 2 2v5zm-2 7a2 2 0 0 1 2 2 2 2 0 0 1-2 2 2 2 0 0 1-2-2v-2h2zm0-1a2 2 0 0 1-2-2 2 2 0 0 1 2-2h5a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-5z" />
        </svg>
      </span>
    )
  }

  // GitHub
  if (clean.includes('github')) {
    return (
      <span
        title="GitHub"
        className={`inline-flex shrink-0 items-center justify-center rounded-full bg-[#24292e] text-white shadow-sm ${sizeClasses} ${className}`}
      >
        <svg viewBox="0 0 24 24" fill="currentColor" className={svgSizes}>
          <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z" />
        </svg>
      </span>
    )
  }

  // Terminal / Console
  if (
    clean.includes('terminal') ||
    clean.includes('powershell') ||
    clean.includes('cmd') ||
    clean.includes('bash') ||
    clean.includes('zsh') ||
    clean.includes('wezterm') ||
    clean.includes('alacritty')
  ) {
    return (
      <span
        title={appName}
        className={`inline-flex shrink-0 items-center justify-center rounded-full bg-black ring-1 ring-white/20 text-emerald-400 font-mono font-bold shadow-sm ${sizeClasses} ${fontSizes} ${className}`}
      >
        &gt;
      </span>
    )
  }

  // Figma
  if (clean.includes('figma')) {
    return (
      <span
        title="Figma"
        className={`inline-flex shrink-0 items-center justify-center rounded-full bg-[#1e1e1e] shadow-sm ${sizeClasses} ${className}`}
      >
        <svg viewBox="0 0 24 24" className={size === 'sm' ? 'h-2.5 w-2.5' : size === 'md' ? 'h-4 w-4' : 'h-5 w-5'}>
          <circle cx="16" cy="18" r="4" fill="#0ACF83" />
          <circle cx="8" cy="18" r="4" fill="#1ABCFE" />
          <circle cx="8" cy="12" r="4" fill="#A259FF" />
          <circle cx="8" cy="6" r="4" fill="#F24E1E" />
          <circle cx="16" cy="6" r="4" fill="#FF7262" />
        </svg>
      </span>
    )
  }

  // Notion / Obsidian
  if (clean.includes('notion') || clean.includes('obsidian')) {
    return (
      <span
        title={appName}
        className={`inline-flex shrink-0 items-center justify-center rounded-full bg-white text-black font-serif font-black shadow-sm ${sizeClasses} ${fontSizes} ${className}`}
      >
        N
      </span>
    )
  }

  // Generic colored initial badge fallback
  const colors = [
    'bg-brand-500/30 text-brand-200 ring-brand-500/40',
    'bg-sky-500/30 text-sky-200 ring-sky-500/40',
    'bg-amber-500/30 text-amber-200 ring-amber-500/40',
    'bg-emerald-500/30 text-emerald-200 ring-emerald-500/40',
    'bg-purple-500/30 text-purple-200 ring-purple-500/40',
  ]
  const charCode = clean.charCodeAt(0) || 0
  const colorClass = colors[charCode % colors.length]
  const initial = (clean[0] || 'A').toUpperCase()

  return (
    <span
      title={appName}
      className={`inline-flex shrink-0 items-center justify-center rounded-full ring-1 font-bold shadow-sm ${sizeClasses} ${fontSizes} ${colorClass} ${className}`}
    >
      {initial}
    </span>
  )
}
