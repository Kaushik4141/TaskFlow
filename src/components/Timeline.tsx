import { useMemo, useState } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { ActivityIcon, ChevronDownIcon, SparklesIcon } from '@animateicons/react/lucide'
import ReactMarkdown from 'react-markdown'
import type { Rollup } from '../types'
import { useTaskStore } from '../stores/taskStore'
import { useStaggerContainer, useStaggerItem } from '../lib/motion'

/**
 * Pieces-style workstream activity timeline: one card per roll-up, newest
 * first, filterable by workstream. This is the primary view of captured work —
 * the old single-shot Documentation view is secondary.
 */
export default function Timeline({ taskId }: { taskId: string }) {
  const { rollups } = useTaskStore()
  const [workstreamFilter, setWorkstreamFilter] = useState<string | null>(null)

  const taskRollups = useMemo(
    () =>
      rollups
        .filter((rollup) => rollup.taskId === taskId)
        .sort((a, b) => b.windowStart.localeCompare(a.windowStart)),
    [rollups, taskId],
  )

  const workstreams = useMemo(() => {
    const slugs = new Set<string>()
    for (const rollup of taskRollups) {
      slugs.add(rollup.workstreamSlug ?? 'Inbox')
    }
    return [...slugs].sort()
  }, [taskRollups])

  const visible = useMemo(
    () =>
      workstreamFilter
        ? taskRollups.filter((rollup) => (rollup.workstreamSlug ?? 'Inbox') === workstreamFilter)
        : taskRollups,
    [taskRollups, workstreamFilter],
  )

  const containerV = useStaggerContainer(0.05)
  const itemV = useStaggerItem()

  if (taskRollups.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <div className="surface max-w-md rounded-2xl p-8 text-center">
          <div className="relative mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-brand-500/20 to-brand-600/10 ring-1 ring-brand-500/20">
            <ActivityIcon className="h-6 w-6 text-brand-300" />
          </div>
          <h2 className="text-balance text-xl font-semibold tracking-tight text-white">
            No roll-ups yet.
          </h2>
          <p className="mt-3 text-sm leading-6 text-white/50">
            TaskFlow rolls up your activity automatically every few minutes while you work — the
            workstream timeline builds itself.
          </p>
        </div>
      </div>
    )
  }

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="mx-auto max-w-4xl space-y-5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-white/40">
              Workstream Timeline
            </p>
            <p className="mt-1 text-xs text-white/35">
              {taskRollups.length} roll-up{taskRollups.length === 1 ? '' : 's'} · updates itself
            </p>
          </div>
          {workstreams.length > 1 && (
            <div className="flex flex-wrap gap-1.5">
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

        <motion.div variants={containerV} initial="hidden" animate="show" className="space-y-3">
          <AnimatePresence initial={false}>
            {visible.map((rollup) => (
              <motion.div key={rollup.id} variants={itemV} layout>
                <RollupCard rollup={rollup} />
              </motion.div>
            ))}
          </AnimatePresence>
        </motion.div>
      </div>
    </div>
  )
}

function RollupCard({ rollup }: { rollup: Rollup }) {
  const [expanded, setExpanded] = useState(false)
  const keyPoints = useMemo(() => parseJsonArray(rollup.keyPoints), [rollup.keyPoints])
  const apps = useMemo(() => parseJsonArray(rollup.apps), [rollup.apps])
  const workstream = rollup.workstreamSlug ?? 'Inbox'

  return (
    <div className="rounded-2xl border border-white/[0.06] bg-white/[0.015] transition-colors hover:border-white/[0.1]">
      <button
        type="button"
        className="flex w-full items-start gap-4 p-4 text-left"
        onClick={() => setExpanded((value) => !value)}
      >
        {/* Time gutter */}
        <div className="flex w-20 shrink-0 flex-col items-end pt-0.5">
          <span className="text-sm font-semibold tabular-nums text-white/85">
            {formatTime(rollup.windowStart)}
          </span>
          <span className="text-[11px] tabular-nums text-white/35">
            {formatTime(rollup.windowEnd)}
          </span>
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="rounded-full border border-brand-500/30 bg-brand-500/10 px-2 py-0.5 text-[11px] font-semibold text-brand-200">
              {workstream}
            </span>
            {rollup.triggerKind && (
              <span className="rounded-full border border-white/[0.08] bg-white/[0.02] px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-white/40">
                {triggerLabel(rollup.triggerKind)}
              </span>
            )}
            {rollup.aiMode && rollup.aiMode !== 'template' && (
              <span className="inline-flex items-center rounded-full border border-white/[0.08] bg-white/[0.02] px-2 py-0.5 text-[10px] font-medium text-white/45">
                <SparklesIcon className="mr-1 h-2.5 w-2.5" />
                {rollup.aiMode}
              </span>
            )}
            <span className="text-[11px] text-white/30">
              {rollup.eventCount} event{rollup.eventCount === 1 ? '' : 's'}
            </span>
          </div>
          <h3 className="mt-1.5 text-sm font-semibold leading-5 text-white/90">{rollup.title}</h3>
          {!expanded && keyPoints.length > 0 && (
            <p className="mt-1 truncate text-xs leading-5 text-white/45">{keyPoints[0]}</p>
          )}
          {apps.length > 0 && (
            <p className="mt-1 text-[11px] text-white/30">{apps.join(' · ')}</p>
          )}
        </div>

        <ChevronDownIcon
          className={`mt-1 h-4 w-4 shrink-0 text-white/30 transition-transform ${expanded ? 'rotate-180' : ''}`}
        />
      </button>

      <AnimatePresence initial={false}>
        {expanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="overflow-hidden"
          >
            <div className="border-t border-white/[0.06] px-4 py-4 pl-24">
              {keyPoints.length > 0 && (
                <ul className="mb-3 space-y-1">
                  {keyPoints.slice(0, 5).map((point, index) => (
                    <li key={index} className="flex gap-2 text-xs leading-5 text-white/60">
                      <span className="mt-1.5 h-1 w-1 shrink-0 rounded-full bg-brand-400/70" />
                      {point}
                    </li>
                  ))}
                </ul>
              )}
              <div className="prose prose-invert prose-sm max-w-none text-white/70 prose-headings:text-white/85 prose-p:text-white/60 prose-li:text-white/60 prose-strong:text-white/85">
                <ReactMarkdown>{rollup.summaryMd}</ReactMarkdown>
              </div>
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
      className={`rounded-full px-2.5 py-1 text-[11px] font-semibold transition-colors ${
        active
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
