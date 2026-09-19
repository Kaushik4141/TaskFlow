import { useEffect, useMemo, useRef, useState } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import {
  ActivityIcon,
  CheckIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  ClipboardIcon,
  DownloadIcon,
  EllipsisIcon,
  ExternalLinkIcon,
  InfoIcon,
  LayersIcon,
  SparklesIcon,
} from '@animateicons/react/lucide'
import { format, formatDistanceToNow, parseISO } from 'date-fns'
import ReactMarkdown from 'react-markdown'
import type { Event, Rollup } from '../types'
import { useTaskStore } from '../stores/taskStore'
import AppLogo from './AppLogo'
import { useToastStore } from '../stores/toastStore'
import { formatActivityTitle, cleanAppName, cleanWindowTitle } from '../lib/activityFormat'

function ClockSvg({ className = 'h-3.5 w-3.5' }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </svg>
  )
}

interface EventDetailProps {
  rollup: Rollup
  taskTitle?: string
  onBack: () => void
}

export default function EventDetail({ rollup, taskTitle, onBack }: EventDetailProps) {
  const { rollups, selectedTask, events, setSelectedRollupId } = useTaskStore()
  const [copied, setCopied] = useState(false)
  const [exported, setExported] = useState(false)
  const [activeTab, setActiveTab] = useState<'summary' | 'activity'>('summary')
  const [overflowOpen, setOverflowOpen] = useState(false)
  const overflowRef = useRef<HTMLDivElement>(null)

  // Sorted list of rollups for this task to compute prev/next
  const taskRollups = useMemo(
    () =>
      rollups
        .filter((r) => r.taskId === rollup.taskId)
        .sort((a, b) => (b.windowEnd || b.windowStart).localeCompare(a.windowEnd || a.windowStart)),
    [rollups, rollup.taskId],
  )

  const currentIndex = taskRollups.findIndex((r) => r.id === rollup.id)
  const prevRollup = currentIndex > 0 ? taskRollups[currentIndex - 1] : null
  const nextRollup = currentIndex >= 0 && currentIndex < taskRollups.length - 1 ? taskRollups[currentIndex + 1] : null

  // Parsed metadata
  const keyPoints = useMemo(() => parseJsonArray(rollup.keyPoints), [rollup.keyPoints])
  const apps = useMemo(() => parseJsonArray(rollup.apps), [rollup.apps])
  const resources = useMemo(() => parseJsonArray(rollup.resources), [rollup.resources])
  const workstream = rollup.workstreamSlug ?? 'Inbox'
  const cleanTitle = useMemo(() => formatActivityTitle(rollup.title), [rollup.title])

  // Parse summaryMd into structured sections for visual hierarchy
  const parsedSummary = useMemo(() => parseSummaryMd(rollup.summaryMd), [rollup.summaryMd])

  // Merge key points: prefer dedicated JSON field, supplement with parsed markdown ones
  const mergedKeyPoints = useMemo(() => {
    const noiseLines = new Set([
      'Local events were captured successfully.',
      'AI summarization was unavailable; showing a local summary of captured events.',
    ])
    const fromJson = keyPoints.filter((kp) => !noiseLines.has(kp))
    if (fromJson.length > 0) return fromJson
    // Fall back to parsed markdown key points if JSON field was empty
    return parsedSummary.keyPoints.filter((kp) => !noiseLines.has(kp))
  }, [keyPoints, parsedSummary.keyPoints])

  // Time formatting
  const { timeRange, relativeTime, durationStr } = useMemo(() => {
    try {
      const startDate = parseISO(rollup.windowStart)
      const endDate = parseISO(rollup.windowEnd)
      const rel = formatDistanceToNow(endDate || startDate, { addSuffix: true })
      const startStr = format(startDate, 'h:mm a')
      const endStr = format(endDate, 'h:mm a')
      const diffMinutes = Math.max(1, Math.round((endDate.getTime() - startDate.getTime()) / 60000))
      return {
        timeRange: `${startStr} – ${endStr}`,
        relativeTime: rel,
        durationStr: `${diffMinutes} min window`,
      }
    } catch {
      return {
        timeRange: `${rollup.windowStart} – ${rollup.windowEnd}`,
        relativeTime: 'recently',
        durationStr: 'Roll-up window',
      }
    }
  }, [rollup.windowStart, rollup.windowEnd])

  // Filter raw events that occurred in this rollup's time window
  const matchingEvents = useMemo(() => {
    if (!events || events.length === 0) return []
    try {
      const startMs = parseISO(rollup.windowStart).getTime()
      const endMs = parseISO(rollup.windowEnd).getTime()
      if (isNaN(startMs) || isNaN(endMs)) return []

      return events
        .filter((e) => {
          const t = parseISO(e.timestamp).getTime()
          return !isNaN(t) && t >= startMs - 2000 && t <= endMs + 2000
        })
        .sort((a, b) => a.timestamp.localeCompare(b.timestamp))
    } catch {
      return []
    }
  }, [events, rollup.windowStart, rollup.windowEnd])

  // Keyboard navigation: Escape to go back, [ for previous, ] for next
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
        return
      }
      if (e.key === 'Escape') {
        e.preventDefault()
        onBack()
      } else if (e.key === '[' && prevRollup) {
        e.preventDefault()
        setSelectedRollupId(prevRollup.id)
      } else if (e.key === ']' && nextRollup) {
        e.preventDefault()
        setSelectedRollupId(nextRollup.id)
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [onBack, prevRollup, nextRollup, setSelectedRollupId])

  // Close overflow menu on outside click or Escape
  useEffect(() => {
    if (!overflowOpen) return
    const handleClickOutside = (e: MouseEvent) => {
      if (overflowRef.current && !overflowRef.current.contains(e.target as Node)) {
        setOverflowOpen(false)
      }
    }
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setOverflowOpen(false)
      }
    }
    window.addEventListener('mousedown', handleClickOutside)
    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.removeEventListener('mousedown', handleClickOutside)
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [overflowOpen])

  // Copy full Markdown representation
  const handleCopyMarkdown = async () => {
    const mdLines = [
      `# ${cleanTitle}`,
      `> **Workstream**: ${workstream} | **Time**: ${timeRange} (${relativeTime})`,
      '',
      '## Summary',
      rollup.summaryMd,
    ]

    if (keyPoints.length > 0) {
      mdLines.push('', '## Key Highlights', ...keyPoints.map((kp) => `- ${kp}`))
    }

    if (apps.length > 0) {
      mdLines.push('', `**Apps Used**: ${apps.join(', ')}`)
    }

    if (resources.length > 0) {
      mdLines.push('', '## Resources', ...resources.map((r) => `- ${r}`))
    }

    await navigator.clipboard.writeText(mdLines.join('\n'))
    setCopied(true)
    useToastStore.getState().addToast('success', 'Event summary copied to clipboard')
    setTimeout(() => setCopied(false), 2000)
  }

  // Export as Markdown file
  const handleExportMarkdown = () => {
    const safeTitle = (cleanTitle || 'event-summary').toLowerCase().replace(/[^a-z0-9_-]/g, '-')
    const blob = new Blob([rollup.summaryMd], { type: 'text/markdown;charset=utf-8' })
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = `${safeTitle}.md`
    anchor.click()
    URL.revokeObjectURL(url)
    setExported(true)
    useToastStore.getState().addToast('success', 'Exported event markdown')
    setTimeout(() => setExported(false), 2000)
  }

  return (
    <motion.div
      key={rollup.id}
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -8 }}
      transition={{ duration: 0.18, ease: 'easeOut' }}
      className="flex h-full flex-col overflow-y-auto"
    >
      {/* Top Navigation & Breadcrumbs Bar — offset below app header (Row 1 + Row 2 ≈ 68px) */}
      <div className="sticky top-0 z-20 flex items-center justify-between border-b border-white/[0.06] bg-noir-950/90 px-6 py-3 pt-[72px] backdrop-blur-md">
        <div className="flex items-center gap-3 min-w-0">
          <button
            type="button"
            onClick={onBack}
            className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.03] px-2.5 py-1 text-xs font-medium text-white/70 transition-colors hover:border-white/20 hover:bg-white/[0.08] hover:text-white"
            title="Back to Task Documentation (Esc)"
          >
            <ChevronLeftIcon className="h-3.5 w-3.5" />
            <span className="truncate max-w-[140px] sm:max-w-[200px]">{taskTitle ? `${taskTitle}` : 'Documentation'}</span>
            <kbd className="hidden sm:inline-block rounded border border-white/10 bg-black/40 px-1 py-0.2 text-[10px] text-white/40">
              Esc
            </kbd>
          </button>

          <span className="text-white/20">/</span>

          <div className="flex items-center gap-2 truncate">
            <span className="text-xs font-semibold text-brand-300 truncate" title={cleanTitle}>
              {cleanTitle}
            </span>
            <span className="rounded-md border border-white/[0.08] bg-white/[0.04] px-2 py-0.5 text-[10px] font-medium text-white/50">
              {workstream}
            </span>
          </div>
        </div>

        {/* Action Buttons & Prev/Next Rollup Controls */}
        <div className="flex items-center gap-2 shrink-0">
          {/* Prev / Next controls */}
          <div className="flex items-center rounded-lg border border-white/10 bg-white/[0.02] p-0.5">
            <button
              type="button"
              disabled={!prevRollup}
              onClick={() => prevRollup && setSelectedRollupId(prevRollup.id)}
              className="rounded p-1 text-white/50 transition-colors hover:bg-white/[0.06] hover:text-white disabled:opacity-25"
              title="Previous event ([)"
            >
              <ChevronLeftIcon className="h-3.5 w-3.5" />
            </button>
            <span className="px-1.5 text-[10px] tabular-nums text-white/40">
              {currentIndex + 1} / {taskRollups.length}
            </span>
            <button
              type="button"
              disabled={!nextRollup}
              onClick={() => nextRollup && setSelectedRollupId(nextRollup.id)}
              className="rounded p-1 text-white/50 transition-colors hover:bg-white/[0.06] hover:text-white disabled:opacity-25"
              title="Next event (])"
            >
              <ChevronRightIcon className="h-3.5 w-3.5" />
            </button>
          </div>

          {/* Overflow menu for Copy & Export .md */}
          <div className="relative" ref={overflowRef}>
            <button
              type="button"
              aria-label="More actions"
              aria-haspopup="menu"
              aria-expanded={overflowOpen}
              title="More actions"
              onClick={() => setOverflowOpen((prev) => !prev)}
              className={`flex items-center justify-center rounded-lg border px-2 py-1 text-xs font-medium transition-colors ${
                overflowOpen
                  ? 'border-brand-500/40 bg-brand-500/15 text-white ring-1 ring-brand-500/30'
                  : 'border-white/10 bg-white/[0.03] text-white/70 hover:border-white/20 hover:bg-white/[0.08] hover:text-white'
              }`}
            >
              <EllipsisIcon className="h-3.5 w-3.5" />
            </button>

            <AnimatePresence>
              {overflowOpen && (
                <motion.div
                  initial={{ opacity: 0, scale: 0.95, y: 4 }}
                  animate={{ opacity: 1, scale: 1, y: 0 }}
                  exit={{ opacity: 0, scale: 0.95, y: 4 }}
                  transition={{ duration: 0.15 }}
                  role="menu"
                  className="absolute right-0 top-full mt-2 w-44 rounded-xl border border-white/10 bg-noir-900/95 p-1.5 shadow-elevated backdrop-blur-md z-40"
                >
                  <button
                    type="button"
                    role="menuitem"
                    onClick={() => {
                      void handleCopyMarkdown()
                      setOverflowOpen(false)
                    }}
                    className="flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-xs font-medium text-white/85 hover:bg-white/[0.08] hover:text-white transition-colors"
                  >
                    {copied ? <CheckIcon className="h-3.5 w-3.5 text-emerald-400" /> : <ClipboardIcon className="h-3.5 w-3.5 text-white/60" />}
                    <span>{copied ? 'Copied' : 'Copy Markdown'}</span>
                  </button>
                  <button
                    type="button"
                    role="menuitem"
                    onClick={() => {
                      handleExportMarkdown()
                      setOverflowOpen(false)
                    }}
                    className="flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-xs font-medium text-white/85 hover:bg-white/[0.08] hover:text-white transition-colors"
                  >
                    {exported ? <CheckIcon className="h-3.5 w-3.5 text-emerald-400" /> : <DownloadIcon className="h-3.5 w-3.5 text-white/60" />}
                    <span>{exported ? 'Exported' : 'Export .md'}</span>
                  </button>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        </div>
      </div>

      {/* Main Content Body */}
      <div className="flex-1 space-y-6 p-6 max-w-5xl mx-auto w-full">
        {/* Header Title & Metadata Banner */}
        <div className="relative overflow-hidden rounded-2xl border border-white/[0.08] bg-gradient-to-br from-white/[0.03] to-white/[0.005] p-6 shadow-xl">
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="space-y-2 max-w-3xl">
              <div className="flex flex-wrap items-center gap-2">
                <span className="inline-flex items-center gap-1 rounded-md bg-brand-500/15 border border-brand-500/30 px-2.5 py-0.5 text-xs font-medium text-brand-200">
                  <ActivityIcon className="h-3 w-3" />
                  {workstream}
                </span>
                <span className="flex items-center gap-1 text-xs text-white/50">
                  <ClockSvg className="h-3.5 w-3.5 text-white/35" />
                  <span>{timeRange}</span>
                  <span className="text-white/20">·</span>
                  <span className="text-white/40">{durationStr}</span>
                  <span className="text-white/20">·</span>
                  <span className="text-white/40">{relativeTime}</span>
                </span>
              </div>

              <h1 className="text-2xl font-semibold tracking-tight text-white sm:text-3xl leading-snug">
                {cleanTitle}
              </h1>
            </div>

            {/* Badges Stack */}
            <div className="flex flex-wrap items-center gap-1.5 shrink-0">
              {rollup.triggerKind && (
                <span className="rounded-full border border-white/10 bg-white/[0.04] px-2.5 py-1 text-[11px] font-medium text-white/60">
                  Trigger: {triggerLabel(rollup.triggerKind)}
                </span>
              )}
              {rollup.aiMode && rollup.aiMode !== 'template' && (
                <span className="inline-flex items-center rounded-full border border-brand-500/20 bg-brand-500/10 px-2.5 py-1 text-[11px] font-medium text-brand-300">
                  <SparklesIcon className="mr-1.5 h-3 w-3" />
                  {rollup.aiMode}
                </span>
              )}
              <span className="rounded-full border border-white/10 bg-white/[0.04] px-2.5 py-1 text-[11px] font-medium text-white/60">
                {rollup.eventCount} window event{rollup.eventCount === 1 ? '' : 's'}
              </span>
            </div>
          </div>
        </div>

        {/* View Switcher: Summary & Details vs Raw Activity Timeline */}
        <div className="flex items-center gap-2 border-b border-white/[0.06] pb-2">
          <button
            type="button"
            onClick={() => setActiveTab('summary')}
            className={`flex items-center gap-2 rounded-lg px-3 py-1.5 text-xs font-semibold transition-colors ${
              activeTab === 'summary'
                ? 'bg-brand-500/20 text-brand-200 ring-1 ring-brand-500/30'
                : 'text-white/50 hover:bg-white/[0.04] hover:text-white'
            }`}
          >
            <SparklesIcon className="h-3.5 w-3.5" />
            <span>Summary & Insights</span>
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('activity')}
            className={`flex items-center gap-2 rounded-lg px-3 py-1.5 text-xs font-semibold transition-colors ${
              activeTab === 'activity'
                ? 'bg-brand-500/20 text-brand-200 ring-1 ring-brand-500/30'
                : 'text-white/50 hover:bg-white/[0.04] hover:text-white'
            }`}
          >
            <ActivityIcon className="h-3.5 w-3.5" />
            <span>
              Captured Activity {matchingEvents.length > 0 && `(${matchingEvents.length})`}
            </span>
          </button>
        </div>

        {activeTab === 'summary' ? (
          <div className="space-y-6">
            {/* Description & AI-notice badge */}
            {(parsedSummary.description || parsedSummary.isLocalFallback) && (
              <div className="rounded-2xl border border-white/[0.08] bg-black/40 p-6 shadow-md">
                <div className="mb-4 flex items-center justify-between">
                  <h2 className="flex items-center gap-2 text-sm font-semibold uppercase tracking-[0.1em] text-white/70">
                    <SparklesIcon className="h-4 w-4 text-brand-400" />
                    <span>Event Summary</span>
                  </h2>
                  {parsedSummary.isLocalFallback && (
                    <span className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/20 bg-amber-500/10 px-2.5 py-0.5 text-[11px] font-medium text-amber-300/80">
                      <InfoIcon className="h-3 w-3" />
                      Local summary — AI unavailable
                    </span>
                  )}
                </div>
                {parsedSummary.description && (
                  <p className="text-sm leading-relaxed text-white/50">{parsedSummary.description}</p>
                )}
                {parsedSummary.summaryText && (
                  <div className="mt-3 prose prose-invert prose-neutral max-w-none text-white/80 leading-relaxed text-sm">
                    <ReactMarkdown>{parsedSummary.summaryText}</ReactMarkdown>
                  </div>
                )}
              </div>
            )}

            {/* Parsed Activity Timeline from summaryMd (fallback summaries) */}
            {parsedSummary.activityLines.length > 0 && (
              <div className="rounded-2xl border border-white/[0.08] bg-black/40 p-6 shadow-md">
                <div className="mb-4 flex items-center justify-between">
                  <h3 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.1em] text-white/60">
                    <ActivityIcon className="h-3.5 w-3.5 text-white/40" />
                    <span>Activity Timeline</span>
                  </h3>
                  <span className="rounded-full bg-white/[0.06] px-2.5 py-1 text-[10px] font-medium text-white/50 tabular-nums">
                    {parsedSummary.activityLines.length} events
                  </span>
                </div>
                <div className="divide-y divide-white/[0.04]">
                  {parsedSummary.activityLines.map((line, idx) => (
                    <SummaryActivityRow key={idx} line={line} />
                  ))}
                </div>
              </div>
            )}

            {/* AI-generated narrative (non-fallback summaries with no structured sections) */}
            {!parsedSummary.isLocalFallback && !parsedSummary.description && parsedSummary.otherContent && (
              <div className="rounded-2xl border border-white/[0.08] bg-black/40 p-6 shadow-md">
                <div className="mb-4 flex items-center justify-between">
                  <h2 className="flex items-center gap-2 text-sm font-semibold uppercase tracking-[0.1em] text-white/70">
                    <SparklesIcon className="h-4 w-4 text-brand-400" />
                    <span>Event Summary</span>
                  </h2>
                </div>
                <div className="prose prose-invert prose-neutral max-w-none text-white/80 leading-relaxed text-sm">
                  <ReactMarkdown>{parsedSummary.otherContent}</ReactMarkdown>
                </div>
              </div>
            )}

            {/* Key Highlights / Points */}
            {mergedKeyPoints.length > 0 && (
              <div className="rounded-2xl border border-white/[0.08] bg-black/40 p-6 shadow-md">
                <h3 className="mb-3 text-xs font-semibold uppercase tracking-[0.1em] text-white/60">
                  Key Takeaways
                </h3>
                <ul className="space-y-2.5">
                  {mergedKeyPoints.map((point, idx) => (
                    <li key={idx} className="flex items-start gap-2.5 text-xs leading-relaxed text-white/75">
                      <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-brand-400" />
                      <span>{point}</span>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {/* 2-Column Grid: Applications & Resources */}
            <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
              {/* Applications & Tools Used */}
              <div className="rounded-2xl border border-white/[0.08] bg-black/40 p-5 shadow-md">
                <h3 className="mb-3.5 flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.1em] text-white/60">
                  <LayersIcon className="h-3.5 w-3.5 text-white/40" />
                  <span>Applications & Windows</span>
                </h3>
                {apps.length === 0 ? (
                  <p className="text-xs text-white/35">No explicit application tags recorded.</p>
                ) : (
                  <div className="flex flex-wrap gap-2">
                    {apps.map((app, idx) => (
                      <div
                        key={`${app}-${idx}`}
                        className="flex items-center gap-2 rounded-xl border border-white/[0.08] bg-white/[0.03] px-3 py-1.5 transition-colors hover:border-white/20 hover:bg-white/[0.06]"
                      >
                        <AppLogo appName={app} size="md" />
                        <span className="text-xs font-medium text-white/80">{app}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              {/* Resources & Links */}
              <div className="rounded-2xl border border-white/[0.08] bg-black/40 p-5 shadow-md">
                <h3 className="mb-3.5 flex items-center gap-2 text-xs font-semibold uppercase tracking-[0.1em] text-white/60">
                  <ExternalLinkIcon className="h-3.5 w-3.5 text-white/40" />
                  <span>Referenced Resources</span>
                </h3>
                {resources.length === 0 ? (
                  <p className="text-xs text-white/35">No external resources or URLs referenced in this interval.</p>
                ) : (
                  <div className="space-y-2">
                    {resources.map((res, idx) => {
                      const { title, url, hostname, favicon } = parseResourceLink(res)
                      return (
                        <a
                          key={`${res}-${idx}`}
                          href={url}
                          target="_blank"
                          rel="noreferrer"
                          className="flex items-center justify-between gap-3 rounded-xl border border-white/[0.06] bg-white/[0.02] px-3 py-2 text-xs transition-all hover:border-brand-500/30 hover:bg-white/[0.05]"
                        >
                          <div className="flex items-center gap-2.5 min-w-0">
                            <img
                              src={favicon}
                              alt=""
                              className="h-4 w-4 shrink-0 rounded object-contain"
                              onError={(e) => {
                                (e.currentTarget as HTMLElement).style.display = 'none'
                              }}
                            />
                            <div className="truncate">
                              <p className="font-medium text-white/85 truncate">{title}</p>
                              <p className="text-[10px] text-white/40 truncate">{hostname}</p>
                            </div>
                          </div>
                          <ExternalLinkIcon className="h-3 w-3 shrink-0 text-white/30" />
                        </a>
                      )
                    })}
                  </div>
                )}
              </div>
            </div>
          </div>
        ) : (
          /* Captured Activity Timeline Tab */
          <div className="rounded-2xl border border-white/[0.08] bg-black/40 p-6 shadow-md">
            <div className="mb-4 flex items-center justify-between">
              <div>
                <h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-white/70">
                  Raw Activity Stream
                </h3>
                <p className="mt-1 text-xs text-white/40">
                  Window focus changes, clipboards, and interactions captured during this event window.
                </p>
              </div>
              <span className="rounded-full bg-white/[0.06] px-2.5 py-1 text-xs font-medium text-white/60 tabular-nums">
                {matchingEvents.length} records
              </span>
            </div>

            {matchingEvents.length === 0 ? (
              <div className="py-12 text-center text-xs text-white/40">
                <ActivityIcon className="mx-auto mb-3 h-8 w-8 text-white/20" />
                <p className="font-medium text-white/60">No raw event rows found in memory</p>
                <p className="mt-1 text-white/35 max-w-sm mx-auto">
                  Raw event content may have been cleared by your hourly retention policy ({rollup.eventCount} total events were processed into this roll-up).
                </p>
              </div>
            ) : (
              <div className="divide-y divide-white/[0.04]">
                {matchingEvents.map((event) => (
                  <RawEventRow key={event.id} event={event} />
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </motion.div>
  )
}

function RawEventRow({ event }: { event: Event }) {
  const [expanded, setExpanded] = useState(false)
  const timeStr = useMemo(() => {
    try {
      return format(parseISO(event.timestamp), 'HH:mm:ss')
    } catch {
      return event.timestamp
    }
  }, [event.timestamp])

  return (
    <div className="py-2.5 text-xs transition-colors hover:bg-white/[0.015]">
      <div
        className="flex items-center justify-between gap-3 cursor-pointer"
        onClick={() => event.content && setExpanded(!expanded)}
      >
        <div className="flex items-center gap-2.5 min-w-0">
          <span className="font-mono text-[11px] text-white/35 tabular-nums shrink-0">{timeStr}</span>
          <AppLogo appName={cleanAppName(event.appName || 'app')} size="sm" />
          <span className="font-medium text-white/80 shrink-0">{cleanAppName(event.appName || 'Unknown')}</span>
          <span className="text-white/20 shrink-0">·</span>
          <span className="truncate text-white/60">{cleanWindowTitle(event.windowTitle) || '(No window title)'}</span>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          <span className="rounded bg-white/[0.05] border border-white/[0.06] px-1.5 py-0.5 text-[10px] uppercase tracking-wider text-white/45">
            {event.eventType}
          </span>
          {event.content && (
            <span className="text-[10px] text-brand-300/80 hover:text-brand-300 underline">
              {expanded ? 'Hide text' : 'View text'}
            </span>
          )}
        </div>
      </div>

      {expanded && event.content && (
        <div className="mt-2 rounded-lg border border-white/[0.06] bg-black/50 p-2.5 text-[11px] font-mono text-white/70 whitespace-pre-wrap max-h-48 overflow-y-auto">
          {event.content}
        </div>
      )}
    </div>
  )
}

function parseJsonArray(raw: string | null): string[] {
  if (!raw) return []
  try {
    const parsed: unknown = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === 'string') : []
  } catch {
    return []
  }
}

function parseResourceLink(raw: string): { title: string; url: string; hostname: string; favicon: string } {
  try {
    const trimmed = raw.trim()
    const url = trimmed.startsWith('http') ? trimmed : `https://${trimmed}`
    const urlObj = new URL(url)
    const hostname = urlObj.hostname.replace(/^www\./, '')
    const favicon = `https://www.google.com/s2/favicons?domain=${hostname}&sz=32`

    let title = hostname
    if (hostname.includes('github.com')) {
      const parts = urlObj.pathname.split('/').filter(Boolean)
      if (parts.length >= 2) title = `${parts[0]}/${parts[1]}`
    } else {
      const parts = urlObj.pathname.split('/').filter(Boolean)
      if (parts.length > 0) {
        title = decodeURIComponent(parts[parts.length - 1].replace(/[-_]/g, ' '))
      }
    }
    return { title, url, hostname, favicon }
  } catch {
    return {
      title: raw,
      url: raw,
      hostname: raw,
      favicon: '',
    }
  }
}

function triggerLabel(trigger: Rollup['triggerKind'] & string): string {
  switch (trigger) {
    case 'interval':
      return 'Auto interval'
    case 'idle':
      return 'Idle flush'
    case 'context_switch':
      return 'Context switch'
    case 'stop':
      return 'Task stopped'
    case 'manual':
      return 'Manual snapshot'
    default:
      return trigger
  }
}

// ---------- Structured summary parsing ----------

interface ActivityLine {
  appName: string
  timestamp: string
  detail: string
}

interface ParsedSummary {
  /** Prose description text (before first ## heading), stripped of redundant # title */
  description: string
  /** Content of the "## Summary" section */
  summaryText: string
  /** Parsed "## Activity Timeline" entries */
  activityLines: ActivityLine[]
  /** Parsed "## Key Points" bullet items */
  keyPoints: string[]
  /** Whether this is a local fallback summary (AI unavailable) */
  isLocalFallback: boolean
  /** Any remaining markdown not matching known sections */
  otherContent: string
}

const FALLBACK_MARKERS = [
  'AI summarization was unavailable',
  'AI documentation was not available',
  'TaskFlow generated a structured local event summary',
]

/**
 * Parse `rollup.summaryMd` into structured sections.
 *
 * Fallback summaries from `commands.rs::fallback_summary()` follow a rigid
 * `# Title / **Description:** / ## Summary / ## Activity Timeline / ## Key Points / ## Resources Referenced`
 * structure. AI-generated summaries have free-form narrative. The parser
 * handles both gracefully.
 */
function parseSummaryMd(md: string): ParsedSummary {
  const result: ParsedSummary = {
    description: '',
    summaryText: '',
    activityLines: [],
    keyPoints: [],
    isLocalFallback: false,
    otherContent: '',
  }

  if (!md || md.trim().length === 0) return result

  // Detect fallback
  result.isLocalFallback = FALLBACK_MARKERS.some((marker) => md.includes(marker))

  // Split into sections by ## headings
  const sectionRegex = /^## (.+)$/gm
  const sections: { heading: string; body: string }[] = []
  let preamble = ''
  let lastIdx = 0
  let lastHeading = ''

  // Find all ## headings
  let match: RegExpExecArray | null
  while ((match = sectionRegex.exec(md)) !== null) {
    const chunk = md.slice(lastIdx, match.index).trim()
    if (lastIdx === 0) {
      preamble = chunk
    } else {
      sections.push({ heading: lastHeading, body: chunk })
    }
    lastHeading = match[1].trim()
    lastIdx = match.index + match[0].length
  }
  // Last section
  if (lastHeading) {
    sections.push({ heading: lastHeading, body: md.slice(lastIdx).trim() })
  } else {
    // No ## headings at all — this is a pure narrative AI summary
    result.otherContent = md.trim()
    return result
  }

  // Parse preamble: strip "# Title" line and extract description prose
  if (preamble) {
    const lines = preamble.split('\n').filter((l) => l.trim().length > 0)
    const descLines: string[] = []
    for (const line of lines) {
      // Skip redundant "# Title" header (already in the banner)
      if (/^#\s+/.test(line)) continue
      // Extract description/event count as description text
      const cleaned = line.replace(/^\*\*Description:\*\*\s*/, '').replace(/^\*\*Events captured:\*\*\s*\d+$/, '').trim()
      if (cleaned) descLines.push(cleaned)
    }
    result.description = descLines.join(' ').trim()
  }

  // Process each section
  const otherParts: string[] = []

  for (const section of sections) {
    const headingLower = section.heading.toLowerCase()

    if (headingLower === 'summary') {
      // Strip the fallback notice from the displayed summary text
      const cleanedLines = section.body
        .split('\n')
        .filter((line) => !FALLBACK_MARKERS.some((m) => line.includes(m)))
        .join('\n')
        .trim()
      result.summaryText = cleanedLines
    } else if (headingLower === 'activity timeline') {
      // Parse each "- **appName** `timestamp`: detail" line
      const lines = section.body.split('\n').filter((l) => l.trim().startsWith('-'))
      for (const line of lines) {
        const parsed = parseActivityLine(line)
        if (parsed) result.activityLines.push(parsed)
      }
    } else if (headingLower === 'key points') {
      const items = section.body
        .split('\n')
        .filter((l) => l.trim().startsWith('-'))
        .map((l) => l.replace(/^-\s*/, '').trim())
        .filter((l) => l.length > 0)
      result.keyPoints = items
    } else if (headingLower === 'resources referenced') {
      // Resources are handled by the dedicated `resources` JSON field + existing card — skip
    } else {
      // Unknown section — include in otherContent
      otherParts.push(`## ${section.heading}\n${section.body}`)
    }
  }

  if (otherParts.length > 0) {
    result.otherContent = otherParts.join('\n\n').trim()
  }

  return result
}

/**
 * Parse a single activity line from the fallback summary format:
 * `- **appName** \`timestamp\`: detail text`
 */
function parseActivityLine(line: string): ActivityLine | null {
  const trimmed = line.replace(/^-\s*/, '').trim()
  // Match: **appName** `timestamp`: detail
  const m = trimmed.match(/^\*\*(.+?)\*\*\s*`(.+?)`:\s*(.*)$/)
  if (m) {
    return {
      appName: m[1].trim(),
      timestamp: m[2].trim(),
      detail: m[3].trim(),
    }
  }
  // Fallback: treat the whole line as detail with unknown app
  if (trimmed.length > 0) {
    return {
      appName: 'Unknown',
      timestamp: '',
      detail: trimmed,
    }
  }
  return null
}

/**
 * Renders a single activity entry from the parsed summaryMd Activity Timeline,
 * styled consistently with `RawEventRow` (AppLogo, time, detail).
 */
function SummaryActivityRow({ line }: { line: ActivityLine }) {
  const timeStr = useMemo(() => {
    if (!line.timestamp) return ''
    try {
      return format(parseISO(line.timestamp), 'HH:mm:ss')
    } catch {
      return line.timestamp
    }
  }, [line.timestamp])

  return (
    <div className="py-2.5 text-xs transition-colors hover:bg-white/[0.015]">
      <div className="flex items-center gap-2.5 min-w-0">
        {timeStr && (
          <span className="font-mono text-[11px] text-white/35 tabular-nums shrink-0">{timeStr}</span>
        )}
        <AppLogo appName={cleanAppName(line.appName)} size="sm" />
        <span className="font-medium text-white/80 shrink-0">{cleanAppName(line.appName)}</span>
        <span className="text-white/20 shrink-0">·</span>
        <span className="truncate text-white/60">{cleanWindowTitle(line.detail) || '(No detail)'}</span>
      </div>
    </div>
  )
}
