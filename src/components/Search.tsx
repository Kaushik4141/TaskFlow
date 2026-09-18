import { useCallback, useEffect, useRef, useState, MouseEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { AnimatePresence, motion } from 'framer-motion'
import { ActivityIcon, BookOpenTextIcon, SearchIcon as SearchIcon, XIcon } from '@animateicons/react/lucide'
import type { SearchResult } from '../types'
import { modalVariants, useStaggerContainer, useStaggerItem } from '../lib/motion'

interface SearchProps {
  onClose: () => void
  onNavigate: (taskId: string) => void
}

export default function Search({ onClose, onNavigate }: SearchProps) {
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<SearchResult[]>([])
  const [loading, setLoading] = useState(false)
  const [searched, setSearched] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)
  const timerRef = useRef<number | null>(null)

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  const search = useCallback(async (q: string) => {
    if (!q.trim()) {
      setResults([])
      setSearched(false)
      return
    }
    setLoading(true)
    try {
      const data = await invoke<SearchResult[]>('search_documentation', { query: q.trim() })
      setResults(data)
      setSearched(true)
    } catch (err) {
      console.error('Search failed:', err)
      setResults([])
      setSearched(true)
    } finally {
      setLoading(false)
    }
  }, [])

  const handleInput = (value: string) => {
    setQuery(value)
    if (timerRef.current) window.clearTimeout(timerRef.current)
    timerRef.current = window.setTimeout(() => void search(value), 300)
  }

  const handleSelect = (taskId: string) => {
    onNavigate(taskId)
    onClose()
  }

  const sourceBadge = (source: string) => {
    const colors: Record<string, string> = {
      jira: 'bg-sky-500/20 text-sky-300',
      github: 'bg-white/10 text-white/70',
      linear: 'bg-violet-500/20 text-violet-300',
      manual: 'bg-brand-500/15 text-brand-200',
    }
    return colors[source] ?? colors.manual
  }

  const containerV = useStaggerContainer(0.035)
  const itemV = useStaggerItem()

  return (
    <>
      <motion.div
        variants={modalVariants}
        initial="hidden"
        animate="visible"
        exit="exit"
        className="fixed inset-0 z-[80] bg-black/70 backdrop-blur-sm"
        onClick={onClose}
      />
      <motion.div
        variants={modalVariants}
        initial="hidden"
        animate="visible"
        exit="exit"
        className="fixed inset-0 z-[81] flex items-start justify-center p-4 pt-[15vh]"
        onClick={onClose}
      >
        <div
          className="w-full max-w-2xl overflow-hidden rounded-2xl border border-white/10 bg-noir-900/95 shadow-elevated backdrop-blur-xl"
          onClick={(e: MouseEvent) => e.stopPropagation()}
        >
          {/* Search Input */}
          <div className="flex items-center gap-3 border-b border-white/[0.06] px-5 py-4">
            <SearchIcon className="h-5 w-5 text-brand-400" />
            <input
              ref={inputRef}
              className="min-w-0 flex-1 bg-transparent text-lg text-white outline-none placeholder:text-white/30"
              placeholder="Search documented tasks..."
              value={query}
              onChange={(e) => handleInput(e.target.value)}
            />
            <kbd className="rounded-md border border-white/10 bg-white/5 px-2 py-0.5 text-xs text-white/40">Esc</kbd>
          </div>

          {/* Results */}
          <div className="max-h-[50vh] overflow-y-auto">
            {!searched && !loading && (
              <div className="flex flex-col items-center gap-3 py-12 text-white/40">
                <BookOpenTextIcon className="h-10 w-10 text-white/15" />
                <p className="text-sm">Search across all your documented tasks</p>
              </div>
            )}

            {loading && (
              <div className="flex items-center justify-center py-12">
                <div className="h-6 w-6 animate-spin rounded-full border-2 border-brand-500/40 border-t-brand-500" />
              </div>
            )}

            {searched && !loading && results.length === 0 && (
              <div className="flex flex-col items-center gap-3 py-12 text-white/40">
                <XIcon className="h-10 w-10 text-white/15" />
                <p className="text-sm">No tasks found matching &lsquo;{query}&rsquo;</p>
              </div>
            )}

            {!loading && results.length > 0 && (
              <motion.div variants={containerV} initial="hidden" animate="show" className="p-2">
                {results.map((result) => (
                  <motion.button
                    key={`${result.taskId}-${result.createdAt}`}
                    variants={itemV}
                    whileHover={{ x: 2 }}
                    whileTap={{ scale: 0.99 }}
                    className="flex w-full items-start gap-3 rounded-xl p-3 text-left transition-colors hover:bg-white/[0.04]"
                    onClick={() => handleSelect(result.taskId)}
                    type="button"
                  >
                    <BookOpenTextIcon className="mt-0.5 h-5 w-5 shrink-0 text-brand-400/70" />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <p className="truncate text-sm font-semibold text-white/90">{result.taskTitle}</p>
                        <span className={`shrink-0 rounded-md px-2 py-0.5 text-[10px] uppercase tracking-wider ${sourceBadge(result.source)}`}>
                          {result.source}
                        </span>
                      </div>
                      {result.matchedSnippet && (
                        <p className="mt-1 line-clamp-2 text-xs leading-5 text-white/45">{result.matchedSnippet}</p>
                      )}
                      <div className="mt-1 flex items-center gap-1 text-xs text-white/30">
                        <ActivityIcon className="h-3 w-3" />
                        {new Date(result.createdAt).toLocaleDateString()}
                      </div>
                    </div>
                    <span className="mt-1 shrink-0 font-mono text-xs text-brand-300">
                      {Math.round(result.relevanceScore * 100)}%
                    </span>
                  </motion.button>
                ))}
              </motion.div>
            )}
          </div>
        </div>
      </motion.div>
    </>
  )
}
