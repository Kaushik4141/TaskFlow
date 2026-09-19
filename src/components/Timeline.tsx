import { useMemo, useState } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import {
  ActivityIcon,
  ChevronDownIcon,
  ChevronLeftIcon,
  SlidersHorizontalIcon,
  SparklesIcon,
  XIcon,
} from '@animateicons/react/lucide'
import { format, formatDistanceToNow, isToday, isYesterday, parseISO } from 'date-fns'
import ReactMarkdown from 'react-markdown'
import type { Rollup } from '../types'
import { useTaskStore } from '../stores/taskStore'
import { selectionSpring, useStaggerContainer, useStaggerItem } from '../lib/motion'
import AppLogo from './AppLogo'
import TaskFlowLogo from './TaskFlowLogo'

interface TimelineProps {
  taskId: string
  onBack?: () => void
}

/**
 * Pieces OS-inspired Workstream Activity Timeline:
 * Rendered in the left sidebar with app logos, workstream pills,
 * date-grouping accordion headers, and instant timeline search.
 */
export default function Timeline({ taskId, onBack }: { taskId: string; onBack?: () => void }) {
  const { rollups, selectedTask, activeTask, selectedRollupId, setSelectedRollupId } = useTaskStore()
  const [searchQuery, setSearchQuery] = useState('')
  const [workstreamFilter, setWorkstreamFilter] = useState<string | null>(null)
  const [collapsedDates, setCollapsedDates] = useState<Record<string, boolean>>({})

  // Rollups for this task, newest first
  const taskRollups = useMemo(
    () =>
      rollups
        .filter((rollup) => rollup.taskId === taskId)
        .sort((a, b) => (b.windowEnd || b.windowStart).localeCompare(a.windowEnd || a.windowStart)),
    [rollups, taskId],
  )

  // Extract all distinct workstreams
  const workstreams = useMemo(() => {
    const slugs = new Set<string>()
    for (const rollup of taskRollups) {
      slugs.add(rollup.workstreamSlug ?? 'Inbox')
    }
    return [...slugs].sort()
  }, [taskRollups])

  // Filtered by search query & workstream chip
  const visibleRollups = useMemo(() => {
    return taskRollups.filter((rollup) => {
      // Workstream chip filter
      if (workstreamFilter && (rollup.workstreamSlug ?? 'Inbox') !== workstreamFilter) {
        return false
      }
      // Text search query filter
      if (searchQuery.trim()) {
        const query = searchQuery.toLowerCase().trim()
        const titleMatch = rollup.title.toLowerCase().includes(query)
        const summaryMatch = rollup.summaryMd.toLowerCase().includes(query)
        const workstreamMatch = (rollup.workstreamSlug ?? 'Inbox').toLowerCase().includes(query)
        const appsMatch = (rollup.apps ?? '').toLowerCase().includes(query)
        if (!titleMatch && !summaryMatch && !workstreamMatch && !appsMatch) {
          return false
        }
      }
      return true
    })
  }, [taskRollups, workstreamFilter, searchQuery])

  // Group rollups by date (e.g. "Today, May 20th", "Yesterday, May 19th")
  const groupedRollups = useMemo(() => {
    return visibleRollups.reduce<Record<string, Rollup[]>>((groups, rollup) => {
      const date = parseISO(rollup.windowEnd || rollup.windowStart)
      const label = isToday(date)
        ? `Today, ${format(date, 'MMM do')}`
        : isYesterday(date)
          ? `Yesterday, ${format(date, 'MMM do')}`
          : format(date, 'EEEE, MMM do')
      groups[label] = [...(groups[label] ?? []), rollup]
      return groups
    }, {})
  }, [visibleRollups])

  const toggleDate = (label: string) => {
    setCollapsedDates((prev) => ({
      ...prev,
      [label]: !prev[label],
    }))
  }

  const containerV = useStaggerContainer(0.04)
  const itemV = useStaggerItem()
  const isActive = activeTask?.id === taskId

  return (
    <div className="flex h-full flex-col">
      {/* Sidebar Timeline Header */}
      <div className="border-b border-white/[0.06] p-4">
        {/* Navigation & Title */}
        <div className="flex items-center justify-between gap-2">
          {onBack ? (
            <button
              type="button"
              onClick={onBack}
              className="flex items-center gap-1.5 rounded-lg px-2 py-1 -ml-2 text-xs font-semibold text-white/50 transition-colors hover:bg-white/[0.04] hover:text-white"
            >
              <ChevronLeftIcon className="h-4 w-4" />
              <span>Tasks</span>
            </button>
          ) : (
            <div className="flex items-center gap-1.5">
              <TaskFlowLogo size="xs" showWordmark={false} />
              <p className="font-display text-[11px] font-medium uppercase tracking-[0.1em] text-white/50">TaskFlow</p>
            </div>
          )}

          {isActive && (
            <span className="inline-flex items-center gap-1.5 rounded-full bg-brand-500/15 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-brand-200">
              <span className="relative flex h-1.5 w-1.5">
                <span className="absolute inline-flex h-full w-full animate-pulse-ring rounded-full bg-brand-500 opacity-60" />
                <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-brand-500" />
              </span>
              Recording
            </span>
          )}
        </div>

        <div className="mt-1 flex items-baseline justify-between">
          <h2 className="line-clamp-1 text-base font-semibold tracking-tight text-white">
            {selectedTask?.title ?? 'Timeline'}
          </h2>
          <span className="text-[11px] text-white/35 tabular-nums">
            {taskRollups.length} {taskRollups.length === 1 ? 'card' : 'cards'}
          </span>
        </div>

        {/* Pieces OS-style Filter Timeline Search Bar */}
        <div className="mt-3 relative flex items-center rounded-xl border border-white/10 bg-black/40 px-3 py-2 transition-all focus-within:border-brand-500/50 focus-within:ring-2 focus-within:ring-brand-500/20">
          <SlidersHorizontalIcon className="mr-2 h-3.5 w-3.5 text-white/40 shrink-0" />
          <input
            type="text"
            className="min-w-0 flex-1 bg-transparent text-xs text-white outline-none placeholder:text-white/30"
            placeholder="Filter Timeline"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
          {searchQuery && (
            <button
              type="button"
              onClick={() => setSearchQuery('')}
              className="rounded p-0.5 text-white/40 hover:text-white transition-colors"
            >
              <XIcon className="h-3 w-3" />
            </button>
          )}
        </div>

        {/* Workstream Filter Chips (if multiple workstreams) */}
        {workstreams.length > 1 && (
          <div className="mt-2.5 flex flex-wrap gap-1.5 overflow-x-auto pb-0.5">
            <FilterChip
              active={workstreamFilter === null}
              label="All"
              onClick={() => setWorkstreamFilter(null)}
            />
            {workstreams.map((slug) => (
              <FilterChip
                key={slug}
                active={workstreamFilter === slug}
                label={slug}
                onClick={() => setWorkstreamFilter(workstreamFilter === slug ? null : slug)}
              />
            ))}
          </div>
        )}
      </div>

      {/* Cards List Grouped by Date */}
      <div className="flex-1 overflow-y-auto p-3 space-y-3">
        {taskRollups.length === 0 ? (
          <div className="flex flex-col items-center gap-3 px-4 py-12 text-center text-white/40">
            <ActivityIcon className="h-8 w-8 text-white/20" />
            <div>
              <p className="text-xs font-semibold text-white/60">No activity rollups yet</p>
              <p className="mt-1 text-[11px] text-white/35">
                Work summaries will automatically generate every ~10 minutes.
              </p>
            </div>
          </div>
        ) : (
          Object.entries(groupedRollups).map(([dateLabel, dateRollups]) => {
            const isCollapsed = !!collapsedDates[dateLabel]
            return (
              <div key={dateLabel} className="mb-3 last:mb-1">
                {/* Date Section Header */}
                <button
                  type="button"
                  onClick={() => toggleDate(dateLabel)}
                  className="mb-1 flex w-full items-center justify-between rounded-lg px-2 py-1 text-xs font-semibold text-white/70 hover:bg-white/[0.04] transition-colors"
                >
                  <span className="flex items-center gap-2">
                    <span>{dateLabel}</span>
                    <span className="rounded-full bg-white/[0.06] px-1.5 py-0.2 text-[10px] text-white/40 tabular-nums">
                      {dateRollups.length}
                    </span>
                  </span>
                  <ChevronDownIcon
                    className={`h-3.5 w-3.5 text-white/40 transition-transform ${isCollapsed ? '-rotate-90' : ''}`}
                  />
                </button>

                {/* Rollup Cards for this date */}
                {!isCollapsed && (
                  <motion.div variants={containerV} initial="hidden" animate="show" className="space-y-0.5">
                    <AnimatePresence initial={false}>
                      {dateRollups.map((rollup) => (
                        <motion.div key={rollup.id} variants={itemV} layout>
                          <PiecesRollupCard
                            rollup={rollup}
                            isSelected={rollup.id === selectedRollupId}
                            onSelect={() => setSelectedRollupId(rollup.id === selectedRollupId ? null : rollup.id)}
                          />
                        </motion.div>
                      ))}
                    </AnimatePresence>
                  </motion.div>
                )}
              </div>
            )
          })
        )}
      </div>
    </div>
  )
}

/**
 * Pieces OS-style Rollup Row:
 * - Direct on sidebar background, no card borders or heavy panels
 * - Title (medium weight, single line, truncate)
 * - Muted meta line below: workstream dot indicator & snippet
 * - Trailing edge: timestamp on top, Chrome favicon-style app icons below
 * - Selected state: left accent bar + subtle background tint
 */
function PiecesRollupCard({
  rollup,
  isSelected,
  onSelect,
}: {
  rollup: Rollup
  isSelected?: boolean
  onSelect?: () => void
}) {
  const [expanded, setExpanded] = useState(false)
  const keyPoints = useMemo(() => parseJsonArray(rollup.keyPoints), [rollup.keyPoints])
  const apps = useMemo(() => parseJsonArray(rollup.apps), [rollup.apps])
  const workstream = rollup.workstreamSlug ?? 'Inbox'

  // Relative time + exact timestamp
  const timeLabel = useMemo(() => {
    try {
      const date = parseISO(rollup.windowEnd || rollup.windowStart)
      return formatDistanceToNow(date, { addSuffix: true })
    } catch {
      return formatTime(rollup.windowEnd || rollup.windowStart)
    }
  }, [rollup.windowEnd, rollup.windowStart])

  // Snippet preview: first key point or stripped markdown
  const snippet = useMemo(() => {
    if (keyPoints.length > 0) return keyPoints[0]
    return rollup.summaryMd.replace(/^[#*-\s]+/, '').slice(0, 160)
  }, [keyPoints, rollup.summaryMd])

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={() => onSelect?.()}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          onSelect?.()
        }
      }}
      className={`group relative w-full px-2.5 py-2 text-left transition-colors border-b border-white/[0.03] last:border-b-0 rounded-lg outline-none cursor-pointer ${
        isSelected
          ? 'bg-brand-500/[0.08]'
          : 'bg-transparent hover:bg-white/[0.035]'
      }`}
    >
      {/* Left accent bar on selected */}
      {isSelected && (
        <motion.div
          layoutId="rollup-selected-bar"
          transition={selectionSpring}
          className="absolute inset-y-1 left-0 w-[3px] rounded-r-full bg-gradient-to-b from-brand-400 to-brand-600"
        />
      )}

      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          {/* Title: single line, medium weight, truncate with ellipsis */}
          <h3
            className={`truncate text-xs font-medium transition-colors ${
              isSelected ? 'text-white' : 'text-white/90 group-hover:text-white'
            }`}
          >
            {rollup.title}
          </h3>

          {/* Muted meta/description line below: 12px, low-opacity tone */}
          <div className="mt-0.5 flex items-center gap-1.5 text-xs text-white/40">
            <span className="inline-flex items-center gap-1 text-[11px] text-white/50 shrink-0">
              <span className="h-1.5 w-1.5 rounded-full bg-brand-400/60" />
              {workstream}
            </span>
            <span className="text-white/20">•</span>
            <span className="truncate text-white/40">{snippet}</span>
          </div>
        </div>

        {/* Trailing edge: timestamp on top, small circular app icons on bottom */}
        <div className="flex shrink-0 flex-col items-end justify-between self-stretch pt-0.5 pl-1">
          <div className="flex items-center gap-1">
            <span className="text-[10px] tabular-nums text-white/35">{timeLabel}</span>
            <button
              type="button"
              title={expanded ? 'Collapse preview' : 'Expand preview'}
              onClick={(e) => {
                e.stopPropagation()
                setExpanded(!expanded)
              }}
              className="rounded p-0.5 text-white/30 hover:text-white transition-colors"
            >
              <ChevronDownIcon className={`h-3 w-3 transition-transform ${expanded ? 'rotate-180' : ''}`} />
            </button>
          </div>
          {apps.length > 0 && (
            <div className="mt-1 flex items-center -space-x-1">
              {apps.slice(0, 3).map((app, idx) => (
                <AppLogo key={`${app}-${idx}`} appName={app} size="sm" className="h-3.5 w-3.5 ring-1 ring-noir-900" />
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Expandable inline Markdown preview */}
      <AnimatePresence initial={false}>
        {expanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="mt-2 border-t border-white/[0.04] pt-2 text-xs overflow-hidden"
            onClick={(e: React.MouseEvent) => e.stopPropagation()}
          >
            <div className="mb-2 flex flex-wrap items-center gap-1.5 text-[10px] text-white/40">
              {rollup.triggerKind && (
                <span>{triggerLabel(rollup.triggerKind)}</span>
              )}
              {rollup.aiMode && rollup.aiMode !== 'template' && (
                <span className="inline-flex items-center">
                  • <SparklesIcon className="mx-1 h-2.5 w-2.5" /> {rollup.aiMode}
                </span>
              )}
              <span>• {rollup.eventCount} event{rollup.eventCount === 1 ? '' : 's'}</span>
            </div>

            {keyPoints.length > 1 && (
              <ul className="mb-2 space-y-1 pl-1">
                {keyPoints.slice(1, 5).map((point, index) => (
                  <li key={index} className="flex gap-2 text-[11px] leading-4 text-white/60">
                    <span className="mt-1 h-1 w-1 shrink-0 rounded-full bg-brand-400/70" />
                    <span className="line-clamp-2">{point}</span>
                  </li>
                ))}
              </ul>
            )}

            <div className="prose prose-invert prose-xs max-w-none text-white/70 pl-1 border-t border-white/[0.04] pt-2">
              <ReactMarkdown>{rollup.summaryMd}</ReactMarkdown>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}

function FilterChip({
  active,
  label,
  onClick,
}: {
  active: boolean
  label: string
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`rounded-full px-2.5 py-0.5 text-[10px] font-semibold transition-colors ${active
          ? 'border border-brand-500/40 bg-brand-500/15 text-brand-200'
          : 'border border-white/[0.08] bg-white/[0.02] text-white/45 hover:border-white/[0.14] hover:text-white/70'
        }`}
    >
      {label}
    </button>
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

function formatTime(rfc3339: string): string {
  const date = new Date(rfc3339)
  if (Number.isNaN(date.getTime())) return rfc3339
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
}

function triggerLabel(trigger: Rollup['triggerKind'] & string): string {
  switch (trigger) {
    case 'interval':
      return 'auto'
    case 'idle':
      return 'idle flush'
    case 'context_switch':
      return 'switch'
    case 'stop':
      return 'stop'
    default:
      return trigger
  }
}
