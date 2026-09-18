import { useEffect, useRef, useState } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { ActivityIcon, ClipboardIcon, CodeXmlIcon, ExternalLinkIcon, GlobeIcon, BookOpenTextIcon, TerminalIcon } from '@animateicons/react/lucide'
import { format } from 'date-fns'
import { useTaskStore } from '../stores/taskStore'
import type { Event } from '../types'
import { useStaggerContainer, useStaggerItem } from '../lib/motion'

export default function EventFeed({ taskId }: { taskId: string }) {
  const { events, fetchEvents } = useTaskStore()
  const bottomRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    void fetchEvents(taskId)
  }, [fetchEvents, taskId])

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
  }, [events.length])

  const containerV = useStaggerContainer(0.03)

  return (
    <div className="flex h-full flex-col p-6">
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-lg font-semibold tracking-tight text-white">Live Event Feed</h2>
        <span className="rounded-full border border-white/[0.08] bg-white/[0.02] px-3 py-1 text-xs text-white/50">Last 50 events</span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto rounded-2xl border border-white/[0.06] bg-black/20 p-2.5">
        {events.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 py-12">
            <div className="relative flex h-12 w-12 items-center justify-center rounded-2xl bg-white/[0.02] ring-1 ring-white/[0.06]">
              <ActivityIcon className="h-6 w-6 text-white/25" />
            </div>
            <p className="text-sm font-semibold text-white/50">Waiting for activity...</p>
            <p className="text-xs text-white/30">Events will appear here while this task is active.</p>
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

  return (
    <>
      <motion.div
        variants={itemV}
        layout
        whileTap={hasContent ? { scale: 0.995 } : undefined}
        className={`grid cursor-pointer grid-cols-[40px_1fr_auto] items-center gap-3 rounded-xl border border-white/[0.04] bg-white/[0.015] p-3 transition-colors hover:border-white/10 hover:bg-white/[0.03] ${
          faded ? 'opacity-60' : ''
        }`}
        title={tooltip}
        onClick={() => hasContent && setExpanded(!expanded)}
      >
        <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-brand-500/[0.08] text-brand-300 ring-1 ring-brand-500/10">
          {event.eventType === 'window_switch' && event.appName ? (
            <span className="text-sm font-bold">{event.appName[0]?.toUpperCase()}</span>
          ) : (
            <Icon className="h-5 w-5" />
          )}
        </div>
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <p className="truncate text-sm font-semibold text-white/85">{event.appName ?? labelFor(event.eventType)}</p>
            {contentIcon(event.contentType)}
            <span className="rounded-md bg-white/5 px-2 py-0.5 text-[10px] uppercase tracking-wider text-white/40">
              {event.eventType.replace('_', ' ')}
            </span>
            {domain && <span className="rounded-md bg-sky-400/10 px-2 py-0.5 text-[10px] text-sky-200">{domain}</span>}
          </div>
          <p className="truncate text-sm text-white/45">{truncate(title, 60)}</p>
        </div>
        <time className="font-mono text-xs text-white/35">{format(new Date(event.timestamp), 'HH:mm:ss')}</time>
      </motion.div>
      <AnimatePresence>
        {expanded && hasContent && (
          <motion.pre
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            className="ml-12 overflow-hidden whitespace-pre-wrap break-words rounded-xl border border-white/[0.06] bg-black/40 p-3 text-xs text-white/65"
          >
            {event.content}
          </motion.pre>
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

function labelFor(type: Event['eventType']) {
  if (type === 'clipboard') {
    return 'ClipboardIcon'
  }
  if (type === 'note') {
    return 'Note'
  }
  if (type === 'url') {
    return 'URL'
  }
  return 'Window'
}
