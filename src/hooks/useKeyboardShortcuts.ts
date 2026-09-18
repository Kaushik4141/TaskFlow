import { useEffect, useRef } from 'react'

interface ShortcutHandlers {
  onSearch?: () => void
  onNewTask?: () => void
  onToggleTask?: () => void
  onSettings?: () => void
  onEscape?: () => void
  onExport?: () => void
  onHelp?: () => void
}

export function useKeyboardShortcuts(handlers: ShortcutHandlers) {
  const handlersRef = useRef(handlers)
  handlersRef.current = handlers

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement
      const isInput =
        target.tagName === 'INPUT' ||
        target.tagName === 'TEXTAREA' ||
        target.tagName === 'SELECT' ||
        target.isContentEditable

      // Esc always works
      if (e.key === 'Escape') {
        handlersRef.current.onEscape?.()
        return
      }

      // Don't fire shortcuts when typing in inputs
      if (isInput) return

      const mod = e.metaKey || e.ctrlKey

      if (mod && e.key === 'k') {
        e.preventDefault()
        handlersRef.current.onSearch?.()
      } else if (mod && e.key === 'n') {
        e.preventDefault()
        handlersRef.current.onNewTask?.()
      } else if (mod && e.key === 'Enter') {
        e.preventDefault()
        handlersRef.current.onToggleTask?.()
      } else if (mod && e.key === ',') {
        e.preventDefault()
        handlersRef.current.onSettings?.()
      } else if (mod && e.key === 'e') {
        e.preventDefault()
        handlersRef.current.onExport?.()
      } else if (e.key === '?') {
        e.preventDefault()
        handlersRef.current.onHelp?.()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])
}
