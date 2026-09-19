import { useCallback, useEffect, useRef, useState, MouseEvent } from 'react'
import { motion } from 'framer-motion'
import {
  ActivityIcon,
  BookOpenTextIcon,
  BrainIcon,
  SearchIcon as SearchIcon,
  SparklesIcon,
  XIcon,
} from '@animateicons/react/lucide'
import { format, parseISO } from 'date-fns'
import ReactMarkdown from 'react-markdown'
import type { MemorySearchMatch, SearchResult } from '../types'
import { useTaskStore } from '../stores/taskStore'
import { modalVariants, useStaggerContainer, useStaggerItem } from '../lib/motion'
import AppLogo from './AppLogo'
import { formatActivityTitle } from '../lib/activityFormat'

interface SearchProps {
  onClose: () => void
  onNavigate: (taskId: string, rollupId?: string) => void
}

type SearchTab = 'memory' | 'tasks'

const QUICK_QUESTIONS = [
  'What did I work on today?',
  'Which tools or apps did I use?',
  'Recent bug fixes & debugging',
  'Authentication & OAuth work',
  'Database schema migrations',
]

function formatTimeRange(endStr: string): string {
  try {
    const end = parseISO(endStr)
    return format(end, 'MMM d, yyyy · h:mm a')
  } catch {
    return endStr.slice(0, 16).replace('T', ' ')
  }
}

export default function Search({ onClose, onNavigate }: SearchProps) {
  const { searchMemory, searchDocumentation } = useTaskStore()
  const [tab, setTab] = useState<SearchTab>('memory')
  const [query, setQuery] = useState('')

  // Memory search state
  const [memoryResults, setMemoryResults] = useState<MemorySearchMatch[]>([])
  const [memoryAnswer, setMemoryAnswer] = useState<string | null>(null)
  const [scopedCount, setScopedCount] = useState<number>(0)

  // Legacy tasks search state
  const [taskResults, setTaskResults] = useState<SearchResult[]>([])

  const [loading, setLoading] = useState(false)
  const [searched, setSearched] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)
  const timerRef = useRef<number | null>(null)

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  const executeSearch = useCallback(
    async (q: string, activeTab: SearchTab = tab) => {
      const trimmed = q.trim()
      if (!trimmed) {
        setMemoryResults([])
        setMemoryAnswer(null)
        setTaskResults([])
        setSearched(false)
        return
      }

      setLoading(true)
      try {
        if (activeTab === 'memory') {
          const resp = await searchMemory(trimmed)
          setMemoryResults(resp.results)
          setMemoryAnswer(resp.answer)
          setScopedCount(resp.scopedCount)
        } else {
          const data = await searchDocumentation(trimmed)
          setTaskResults(data)
        }
        setSearched(true)
      } catch (err) {
        console.error('Search failed:', err)
        setMemoryResults([])
        setMemoryAnswer(null)
        setTaskResults([])
        setSearched(true)
      } finally {
        setLoading(false)
      }
    },
    [tab, searchMemory, searchDocumentation],
  )

  const handleInput = (value: string) => {
    setQuery(value)
    if (timerRef.current) window.clearTimeout(timerRef.current)
    timerRef.current = window.setTimeout(() => void executeSearch(value, tab), 350)
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      if (timerRef.current) window.clearTimeout(timerRef.current)
      void executeSearch(query, tab)
    }
  }

  const handleQuickQuestion = (question: string) => {
    setTab('memory')
    setQuery(question)
    if (timerRef.current) window.clearTimeout(timerRef.current)
    void executeSearch(question, 'memory')
  }

  const handleTabChange = (newTab: SearchTab) => {
    setTab(newTab)
    if (query.trim()) {
      void executeSearch(query, newTab)
    }
  }

  const handleSelectMemory = (taskId: string, rollupId: string) => {
    onNavigate(taskId, rollupId)
    onClose()
  }

  const handleSelectTask = (taskId: string) => {
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
        className="fixed inset-0 z-[80] bg-black/75 backdrop-blur-md"
        onClick={onClose}
      />
      <motion.div
        variants={modalVariants}
        initial="hidden"
        animate="visible"
        exit="exit"
        className="fixed inset-0 z-[81] flex items-start justify-center p-4 pt-[10vh]"
        onClick={onClose}
      >
        <div
          className="w-full max-w-2xl overflow-hidden rounded-2xl border border-white/10 bg-noir-900/95 shadow-elevated backdrop-blur-xl"
          onClick={(e: MouseEvent) => e.stopPropagation()}
        >
          {/* Header & Tabs */}
          <div className="flex items-center justify-between border-b border-white/[0.06] px-5 pt-3 pb-2">
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => handleTabChange('memory')}
                className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-all ${
                  tab === 'memory'
                    ? 'bg-brand-500/20 text-brand-300 ring-1 ring-brand-500/30'
                    : 'text-white/50 hover:bg-white/[0.04] hover:text-white/80'
                }`}
              >
                <BrainIcon className="h-3.5 w-3.5 text-brand-400" />
                <span>Memory & Questions</span>
              </button>
              <button
                type="button"
                onClick={() => handleTabChange('tasks')}
                className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-all ${
                  tab === 'tasks'
                    ? 'bg-white/10 text-white ring-1 ring-white/20'
                    : 'text-white/50 hover:bg-white/[0.04] hover:text-white/80'
                }`}
              >
                <BookOpenTextIcon className="h-3.5 w-3.5" />
                <span>Documented Tasks</span>
              </button>
            </div>
            <kbd className="rounded-md border border-white/10 bg-white/5 px-2 py-0.5 text-xs text-white/40">
              Esc
            </kbd>
          </div>

          {/* Search Input */}
          <div className="flex items-center gap-3 border-b border-white/[0.06] px-5 py-3.5">
            <SearchIcon className="h-5 w-5 shrink-0 text-brand-400" />
            <input
              ref={inputRef}
              className="min-w-0 flex-1 bg-transparent text-base text-white outline-none placeholder:text-white/30"
              placeholder={
                tab === 'memory'
                  ? 'Ask a question or search memory (e.g., "What did I do today?", "OAuth bug")...'
                  : 'Search documented tasks...'
              }
              value={query}
              onChange={(e) => handleInput(e.target.value)}
              onKeyDown={handleKeyDown}
            />
            {query && (
              <button
                type="button"
                onClick={() => handleInput('')}
                className="text-white/30 hover:text-white/60 p-1"
              >
                <XIcon className="h-4 w-4" />
              </button>
            )}
          </div>

          {/* Quick Questions Chips (when query is empty and in memory tab) */}
          {tab === 'memory' && !query && (
            <div className="border-b border-white/[0.04] bg-white/[0.01] px-5 py-3">
              <div className="mb-2 flex items-center gap-1 text-[11px] font-medium text-white/40">
                <SparklesIcon className="h-3 w-3 text-brand-400" />
                <span>Quick Prompts</span>
              </div>
              <div className="flex flex-wrap gap-1.5">
                {QUICK_QUESTIONS.map((q) => (
                  <button
                    key={q}
                    type="button"
                    onClick={() => handleQuickQuestion(q)}
                    className="rounded-lg border border-white/[0.06] bg-white/[0.03] px-2.5 py-1 text-xs text-white/70 transition-all hover:border-brand-500/40 hover:bg-brand-500/10 hover:text-brand-200"
                  >
                    {q}
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* Results Container */}
          <div className="max-h-[60vh] overflow-y-auto p-4 space-y-3">
            {/* Empty State */}
            {!searched && !loading && !query && (
              <div className="flex flex-col items-center gap-3 py-10 text-white/40 text-center">
                {tab === 'memory' ? (
                  <>
                    <BrainIcon className="h-10 w-10 text-white/15" />
                    <p className="text-sm">
                      Ask anything about your past activity, projects, tools, or notes
                    </p>
                  </>
                ) : (
                  <>
                    <BookOpenTextIcon className="h-10 w-10 text-white/15" />
                    <p className="text-sm">Search across all your documented tasks</p>
                  </>
                )}
              </div>
            )}

            {/* Loading */}
            {loading && (
              <div className="flex items-center justify-center py-12">
                <div className="h-6 w-6 animate-spin rounded-full border-2 border-brand-500/40 border-t-brand-500" />
              </div>
            )}

            {/* No Results */}
            {searched &&
              !loading &&
              ((tab === 'memory' && memoryResults.length === 0 && !memoryAnswer) ||
                (tab === 'tasks' && taskResults.length === 0)) && (
                <div className="flex flex-col items-center gap-3 py-10 text-white/40">
                  <XIcon className="h-10 w-10 text-white/15" />
                  <p className="text-sm">No results found matching &lsquo;{query}&rsquo;</p>
                </div>
              )}

            {/* Memory Tab Results */}
            {tab === 'memory' && !loading && (memoryAnswer || memoryResults.length > 0) && (
              <div className="space-y-3">
                {/* AI Synthesized Answer Card */}
                {memoryAnswer && (
                  <motion.div
                    initial={{ opacity: 0, y: -4 }}
                    animate={{ opacity: 1, y: 0 }}
                    className="rounded-xl border border-brand-500/30 bg-gradient-to-br from-brand-500/10 via-brand-600/5 to-transparent p-4 shadow-sm"
                  >
                    <div className="flex items-center gap-2 mb-2 text-brand-300 font-medium text-xs">
                      <SparklesIcon className="h-4 w-4 text-brand-400" />
                      <span>Memory Engine Answer</span>
                      {scopedCount > 0 && (
                        <span className="ml-auto text-[11px] text-white/40">
                          Synthesized from {scopedCount} memory nodes
                        </span>
                      )}
                    </div>
                    <div className="prose prose-invert prose-sm max-w-none text-white/90 text-sm leading-relaxed [&>p]:mb-2 [&>p:last-child]:mb-0 [&_strong]:text-brand-200 [&_code]:bg-white/10 [&_code]:px-1 [&_code]:rounded">
                      <ReactMarkdown>{memoryAnswer}</ReactMarkdown>
                    </div>
                  </motion.div>
                )}

                {/* Scored Memory Entries */}
                {memoryResults.length > 0 && (
                  <div>
                    <div className="mb-2 px-1 text-[11px] font-medium tracking-wide uppercase text-white/40">
                      Recorded Workstreams & Rollups ({memoryResults.length})
                    </div>
                    <motion.div variants={containerV} initial="hidden" animate="show" className="space-y-2">
                      {memoryResults.map((result) => (
                        <motion.button
                          key={result.rollupId}
                          variants={itemV}
                          whileHover={{ x: 2 }}
                          whileTap={{ scale: 0.99 }}
                          className="group flex w-full flex-col gap-2 rounded-xl border border-white/[0.04] bg-white/[0.02] p-3 text-left transition-all hover:border-brand-500/30 hover:bg-white/[0.05]"
                          onClick={() => handleSelectMemory(result.taskId, result.rollupId)}
                          type="button"
                        >
                          <div className="flex items-center justify-between gap-2">
                            <div className="flex items-center gap-2 min-w-0">
                              <span className="shrink-0 rounded-md bg-brand-500/20 px-2 py-0.5 text-[11px] font-medium text-brand-200">
                                {result.projectSlug ? `[[Projects/${result.projectSlug}]]` : 'Inbox'}
                              </span>
                              <p
                                className="truncate text-sm font-semibold text-white/90 group-hover:text-brand-300 transition-colors"
                                title={formatActivityTitle(result.title)}
                              >
                                {formatActivityTitle(result.title)}
                              </p>
                            </div>
                            <span className="shrink-0 font-mono text-xs text-brand-400/80">
                              {Math.round(result.score * 100)}%
                            </span>
                          </div>

                          {result.summaryMd && (
                            <p className="line-clamp-2 text-xs leading-relaxed text-white/60">
                              {result.summaryMd.replace(/^#+\s+/gm, '')}
                            </p>
                          )}

                          <div className="flex items-center justify-between pt-1 text-[11px] text-white/40 border-t border-white/[0.04]">
                            <div className="flex items-center gap-1.5">
                              <ActivityIcon className="h-3 w-3 text-white/30" />
                              <span>{formatTimeRange(result.windowEnd)}</span>
                            </div>

                            {result.connectedTools.length > 0 && (
                              <div className="flex items-center gap-1">
                                {result.connectedTools.slice(0, 4).map((tool, idx) => (
                                  <span
                                    key={`${tool}-${idx}`}
                                    className="inline-flex items-center gap-1 rounded bg-white/[0.06] px-1.5 py-0.5 text-[10px] text-white/70"
                                  >
                                    <AppLogo appName={tool} size="sm" />
                                    <span>{tool}</span>
                                  </span>
                                ))}
                                {result.connectedTools.length > 4 && (
                                  <span className="text-[10px] text-white/40">
                                    +{result.connectedTools.length - 4}
                                  </span>
                                )}
                              </div>
                            )}
                          </div>
                        </motion.button>
                      ))}
                    </motion.div>
                  </div>
                )}
              </div>
            )}

            {/* Task Tab Results */}
            {tab === 'tasks' && !loading && taskResults.length > 0 && (
              <motion.div variants={containerV} initial="hidden" animate="show" className="space-y-1">
                {taskResults.map((result) => (
                  <motion.button
                    key={`${result.taskId}-${result.createdAt}`}
                    variants={itemV}
                    whileHover={{ x: 2 }}
                    whileTap={{ scale: 0.99 }}
                    className="flex w-full items-start gap-3 rounded-xl p-3 text-left transition-colors hover:bg-white/[0.04]"
                    onClick={() => handleSelectTask(result.taskId)}
                    type="button"
                  >
                    <BookOpenTextIcon className="mt-0.5 h-5 w-5 shrink-0 text-brand-400/70" />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <p className="truncate text-sm font-semibold text-white/90">
                          {result.taskTitle}
                        </p>
                        <span
                          className={`shrink-0 rounded-md px-2 py-0.5 text-[10px] uppercase tracking-wider ${sourceBadge(
                            result.source,
                          )}`}
                        >
                          {result.source}
                        </span>
                      </div>
                      {result.matchedSnippet && (
                        <p className="mt-1 line-clamp-2 text-xs leading-5 text-white/45">
                          {result.matchedSnippet}
                        </p>
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
