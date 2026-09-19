import { useEffect, useRef, useState } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import {
  ActivityIcon,
  BookOpenTextIcon,
  CheckIcon,
  ClipboardIcon,
  CodeXmlIcon,
  ExternalLinkIcon,
  GlobeIcon,
  TerminalIcon,
} from '@animateicons/react/lucide'
import { format } from 'date-fns'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { useTaskStore } from '../stores/taskStore'
import type { Event } from '../types'
import { useStaggerContainer, useStaggerItem } from '../lib/motion'

interface EventFeedProps {
  taskId?: string
  maxHeight?: string
}

/**
 * Developer Options Raw Event Stream:
 * Renders the last 50 raw telemetry events with auto-scroll,
 * app icons, event/content badges, domain tags, timestamps,
 * and click-to-expand raw content.
 */
export default function EventFeed({ taskId, maxHeight = 'max-h-[440px]' }: EventFeedProps) {
  const { events, fetchEvents, tasks, activeTask, selectedTask } = useTaskStore()
  const [selectedTaskId, setSelectedTaskId] = useState<string>(
    taskId || activeTask?.id || selectedTask?.id || tasks[0]?.id || ''
  )
  const [autoScroll, setAutoScroll] = useState(true)
  const bottomRef = useRef<HTMLDivElement | null>(null)

  // Sync taskId if activeTask or tasks become available
  useEffect(() => {
    const currentId = taskId || activeTask?.id || selectedTask?.id || tasks[0]?.id || ''
    if (currentId && !selectedTaskId) {
      setSelectedTaskId(currentId)
    }
  }, [taskId, activeTask?.id, selectedTask?.id, tasks, selectedTaskId])

  // Fetch events on task change
  useEffect(() => {
    if (selectedTaskId) {
      void fetchEvents(selectedTaskId)
    }
  }, [fetchEvents, selectedTaskId])

  // Auto-scroll to bottom on new events
  useEffect(() => {
    if (autoScroll) {
      bottomRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
    }
  }, [events.length, autoScroll])

  const containerV = useStaggerContainer(0.02)

  return (
    <div className="flex flex-col gap-3">
      {/* Controls Bar */}
      <div className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-white/[0.06] bg-white/[0.02] p-2.5">
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-white/40 shrink-0">Task:</span>
          {tasks.length > 0 ? (
            <select
              value={selectedTaskId}
              onChange={(e) => setSelectedTaskId(e.target.value)}
              className="select min-w-0 flex-1 py-1 text-xs truncate bg-black/40 border-white/10"
            >
              {tasks.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.title} {t.status === 'active' ? '(Active)' : ''}
                </option>
              ))}
            </select>
          ) : (
            <span className="text-xs text-white/40 italic">No tasks created</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => setAutoScroll(!autoScroll)}
            className={`rounded-lg px-2 py-1 text-[10px] font-semibold transition-colors ${
              autoScroll
                ? 'bg-brand-500/20 text-brand-200 border border-brand-500/30'
                : 'bg-white/5 text-white/40 hover:text-white/70'
            }`}
          >
            Auto-scroll: {autoScroll ? 'ON' : 'OFF'}
          </button>
          <span className="rounded-full bg-white/[0.06] px-2 py-0.5 text-[10px] font-mono text-white/50">
            {events.length} events
          </span>
        </div>
      </div>

      {/* Constrained Raw Event Stream Container */}
      <div
        className={`min-h-[220px] ${maxHeight} overflow-y-auto rounded-2xl border border-white/[0.06] bg-black/30 p-2`}
      >
        {events.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-3 py-12 text-center">
            <div className="relative flex h-10 w-10 items-center justify-center rounded-xl bg-white/[0.02] ring-1 ring-white/[0.06]">
              <ActivityIcon className="h-5 w-5 text-white/25" />
            </div>
            <p className="text-xs font-semibold text-white/50">Waiting for activity...</p>
            <p className="max-w-xs text-[11px] text-white/30">
              Raw events will appear here as windows change and activity is captured.
            </p>
          </div>
        ) : (
          <motion.div variants={containerV} initial="hidden" animate="show" className="space-y-1.5">
            <AnimatePresence initial={false}>
              {events.map((event) => (
                <EventRow key={event.id} event={event} />
              ))}
            </AnimatePresence>
            <div ref={bottomRef} />
          </motion.div>
        )}
      </div>
    </div>
  )
}

function EventRow({ event }: { event: Event }) {
  const [expanded, setExpanded] = useState(false)
  const [copied, setCopied] = useState(false)
  const itemV = useStaggerItem()
  const Icon = iconFor(event.eventType)
  const title = event.windowTitle || event.content || event.url || event.eventType
  const faded = event.captureMethod === 'title_only'
  const domain = event.url ? domainFor(event.url) : null
  const hasContent = !!event.content
  const tooltip = [
    event.windowTitle ? `Title: ${event.windowTitle}` : null,
    event.url ? `URL: ${event.url}` : null,
    event.content ? `Content: ${event.content.slice(0, 100)}` : null,
    event.captureMethod ? `Capture: ${event.captureMethod}` : null,
  ]
    .filter(Boolean)
    .join('\n')

  const handleCopy = async (e: React.MouseEvent) => {
    e.stopPropagation()
    if (event.content) {
      await writeText(event.content)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    }
  }

  return (
    <>
      <motion.div
        variants={itemV}
        layout
        whileTap={hasContent ? { scale: 0.995 } : undefined}
        className={`grid cursor-pointer grid-cols-[32px_1fr_auto] items-center gap-2.5 rounded-xl border border-white/[0.04] bg-white/[0.015] p-2.5 transition-colors hover:border-white/10 hover:bg-white/[0.03] ${
          faded ? 'opacity-60' : ''
        }`}
        title={tooltip}
        onClick={() => hasContent && setExpanded(!expanded)}
      >
        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-brand-500/[0.08] text-brand-300 ring-1 ring-brand-500/10">
          {event.eventType === 'window_switch' && event.appName && event.appName.toLowerCase() !== 'unknown' ? (
            <span className="text-xs font-semibold">{event.appName[0]?.toUpperCase()}</span>
          ) : (
            <Icon className="h-4 w-4" />
          )}
        </div>
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-1.5">
            <p className="truncate text-xs font-semibold text-white/85">
              {event.appName && event.appName.trim().toLowerCase() !== 'unknown'
                ? event.appName
                : labelFor(event.eventType, event.captureMethod)}
            </p>
            {contentIcon(event.contentType)}
            <span className="rounded bg-white/5 px-1.5 py-0.2 text-[9px] uppercase tracking-wider text-white/40">
              {event.eventType.replace('_', ' ')}
            </span>
            {domain && (
              <span className="rounded bg-sky-400/10 px-1.5 py-0.2 text-[9px] text-sky-200 truncate max-w-[120px]">
                {domain}
              </span>
            )}
          </div>
          <p className="truncate text-xs text-white/45">{truncate(title, 48)}</p>
        </div>
        <div className="flex items-center gap-1.5 shrink-0">
          <time className="font-mono text-[10px] text-white/35 tabular-nums">
            {format(new Date(event.timestamp), 'HH:mm:ss')}
          </time>
        </div>
      </motion.div>

      {/* Expanded raw content */}
      <AnimatePresence>
        {expanded && hasContent && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            className="overflow-hidden"
          >
            <div className="mt-1 rounded-xl border border-white/[0.06] bg-black/40 p-2.5">
              <div className="mb-1.5 flex items-center justify-between text-[10px] text-white/40">
                <span className="font-mono uppercase tracking-wider">Raw Telemetry Content</span>
                <div className="flex items-center gap-2">
                  <span className="font-mono">{event.content?.length ?? 0} chars</span>
                  <button
                    type="button"
                    onClick={handleCopy}
                    className="flex items-center gap-1 rounded px-1.5 py-0.5 text-white/50 hover:bg-white/10 hover:text-white transition-colors"
                  >
                    {copied ? <CheckIcon className="h-3 w-3 text-emerald-400" /> : <ClipboardIcon className="h-3 w-3" />}
                    <span>{copied ? 'Copied' : 'Copy'}</span>
                  </button>
                </div>
              </div>
              <pre className="max-h-48 overflow-y-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-white/70">
                {event.content}
              </pre>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </>
  )
}

function contentIcon(contentType: string | null) {
  if (contentType === 'CodeContent') return <CodeXmlIcon className="h-3.5 w-3.5 text-white/60" />
  if (contentType === 'BrowserContent') return <GlobeIcon className="h-3.5 w-3.5 text-sky-300" />
  if (contentType === 'TerminalContent') return <TerminalIcon className="h-3.5 w-3.5 text-amber-300" />
  return null
}

function domainFor(url: string) {
  try {
    return new URL(url).hostname.replace(/^www\./, '')
  } catch {
    return url.replace(/^https?:\/\//, '').split('/')[0]
  }
}

function truncate(value: string, max: number) {
  return value.length > max ? `${value.slice(0, max - 1)}…` : value
}

function iconFor(type: Event['eventType']) {
  if (type === 'clipboard') {
    return ClipboardIcon
  }
  if (type === 'note') {
    return BookOpenTextIcon
  }
  if (type === 'url') {
    return ExternalLinkIcon
  }
  return CodeXmlIcon
}

function labelFor(type: Event['eventType'], captureMethod?: string | null) {
  if (type === 'clipboard') {
    return 'Clipboard'
  }
  if (type === 'note') {
    return 'Note'
  }
  if (type === 'url') {
    return 'URL'
  }
  if (captureMethod?.toLowerCase().includes('ocr')) {
    return 'Screen OCR'
  }
  return 'Window'
}
