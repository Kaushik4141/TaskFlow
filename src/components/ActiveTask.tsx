import { FormEvent, useEffect, useState } from 'react'
import { motion } from 'framer-motion'
import { differenceInSeconds, formatDistanceStrict, parseISO } from 'date-fns'
import { BookOpenTextIcon, LoaderCircleIcon, PauseIcon, PlayIcon, SparklesIcon } from '@animateicons/react/lucide'
import { useTaskStore } from '../stores/taskStore'
import type { Task } from '../types'
import Documentation from './Documentation'
import { SkeletonDocument } from './Skeleton'
import { useStaggerContainer, useStaggerItem } from '../lib/motion'

export default function ActiveTask({ task }: { task: Task }) {
  const {
    startTask,
    stopTask,
    addNote,
    sidecarReady,
    documentation,
    isGenerating,
    generateDocumentation,
    fetchCaptureStats,
  } = useTaskStore()
  const [note, setNote] = useState('')
  const [now, setNow] = useState(Date.now())

  useEffect(() => {
    if (task.status !== 'active') {
      return
    }
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [task.status])

  useEffect(() => {
    void fetchCaptureStats(task.id)
    const timer = window.setInterval(() => {
      void fetchCaptureStats(task.id)
    }, 10000)
    return () => window.clearInterval(timer)
  }, [fetchCaptureStats, task.id])

  const handleNote = async (event: FormEvent) => {
    event.preventDefault()
    if (!note.trim()) {
      return
    }
    await addNote(task.id, note.trim())
    setNote('')
  }

  const handleTaskToggle = async () => {
    if (task.status === 'active') {
      await stopTask(task.id)
    } else {
      await startTask(task.id)
    }
  }

  const metricsV = useStaggerContainer(0.06)
  const metricV = useStaggerItem()

  return (
    <div className="flex h-full flex-col">
      <header className="relative overflow-hidden border-b border-white/[0.06] p-6">
        {/* Header ambient glow when task is active */}
        {task.status === 'active' && (
          <div
            className="pointer-events-none absolute -right-20 -top-20 h-64 w-64 rounded-full opacity-40 blur-[80px]"
            style={{ background: 'radial-gradient(circle, rgba(255, 59, 71, 0.4), transparent 70%)' }}
          />
        )}
        <div className="relative flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-[11px] font-medium uppercase tracking-[0.18em] text-white/40">{task.source}</span>
              {task.status === 'active' && (
                <span className="inline-flex items-center gap-1.5 rounded-full bg-brand-500/15 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-brand-200">
                  <span className="relative flex h-1.5 w-1.5">
                    <span className="absolute inline-flex h-full w-full animate-pulse-ring rounded-full bg-brand-500 opacity-60" />
                    <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-brand-500" />
                  </span>
                  Recording
                </span>
              )}
            </div>
            <h1 className="mt-1.5 text-balance text-2xl font-semibold tracking-tight text-white">{task.title}</h1>
            {task.description && <p className="mt-2 max-w-3xl text-sm leading-6 text-white/50">{task.description}</p>}
          </div>
          <div className="flex flex-col items-end gap-3">
            <div className="inline-flex items-center gap-2 rounded-xl border border-white/[0.08] bg-white/[0.02] px-3 py-2 text-sm text-white/70">
              <span className={`relative flex h-2.5 w-2.5 ${sidecarReady ? '' : 'opacity-50'}`}>
                {sidecarReady && <span className="absolute inline-flex h-full w-full animate-pulse-ring rounded-full bg-brand-400 opacity-50" />}
                <span className={`relative inline-flex h-2.5 w-2.5 rounded-full ${sidecarReady ? 'bg-brand-500' : 'bg-white/30'}`} />
              </span>
              {sidecarReady ? 'AI ready' : 'AI loading...'}
            </div>
            <div className="flex flex-wrap justify-end gap-2">
              <motion.button
                type="button"
                whileHover={{ y: -1 }}
                whileTap={{ scale: 0.97 }}
                className={`inline-flex items-center gap-2 rounded-xl px-4 py-2.5 font-semibold transition-all disabled:cursor-not-allowed disabled:opacity-60 ${
                  task.status === 'active'
                    ? 'border border-white/10 bg-white/[0.04] text-white/90 hover:border-white/20 hover:bg-white/[0.08] hover:text-white'
                    : 'border border-white/15 bg-white/[0.08] text-white hover:border-white/25 hover:bg-white/[0.12] shadow-elevated'
                }`}
                disabled={isGenerating}
                onClick={() => void handleTaskToggle()}
              >
                {isGenerating ? (
                  <LoaderCircleIcon className="h-[18px] w-[18px] animate-spin" />
                ) : task.status === 'active' ? (
                  <PauseIcon className="h-[18px] w-[18px]" />
                ) : (
                  <PlayIcon className="h-[18px] w-[18px]" />
                )}
                {isGenerating ? 'Generating...' : task.status === 'active' ? 'Stop Task' : 'Start Task'}
              </motion.button>
            </div>
            {isGenerating && (
              <p className="max-w-xs text-right text-xs leading-5 text-white/40">
                Generating with local AI may take a few minutes on first run.
              </p>
            )}
          </div>
        </div>
        <motion.div
          variants={metricsV}
          initial="hidden"
          animate="show"
          className="mt-5 grid grid-cols-1 gap-3 sm:grid-cols-3"
        >
          <motion.div variants={metricV}>
            <Metric label="Status" value={task.status} accent={task.status === 'active'} />
          </motion.div>
          <motion.div variants={metricV}>
            <Metric label="Duration" value={duration(task, now)} />
          </motion.div>
          <motion.div variants={metricV}>
            <Metric label="Created" value={new Date(task.createdAt).toLocaleString()} />
          </motion.div>
        </motion.div>
      </header>

      {/* Primary Workspace View: Documentation */}
      <div className="relative min-h-0 flex-1 overflow-y-auto">
        {isGenerating ? (
          <SkeletonDocument />
        ) : documentation ? (
          <Documentation documentation={documentation} />
        ) : (
          <div className="flex h-full flex-col items-center justify-center p-8 text-center">
            <div className="relative mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-brand-500/20 to-brand-600/10 ring-1 ring-brand-500/20">
              <BookOpenTextIcon className="h-6 w-6 text-brand-300" />
              <div className="absolute inset-0 -z-10 rounded-2xl bg-brand-500/30 blur-xl" />
            </div>
            <h2 className="text-balance text-xl font-semibold tracking-tight text-white">
              No documentation generated yet
            </h2>
            <p className="mt-3 max-w-md text-sm leading-6 text-white/50">
              TaskFlow continuously captures activity and creates roll-ups in your sidebar timeline. Generate
              comprehensive Markdown documentation from your captured work at any time.
            </p>
            <motion.button
              type="button"
              whileHover={{ y: -1 }}
              whileTap={{ scale: 0.98 }}
              className="btn-primary mt-6"
              onClick={() => void generateDocumentation(task.id)}
            >
              <SparklesIcon className="h-4 w-4" />
              Generate Documentation
            </motion.button>
          </div>
        )}
      </div>

      <form className="border-t border-white/[0.06] p-4" onSubmit={handleNote}>
        <div className="flex gap-3">
          <div className="flex h-11 w-11 items-center justify-center rounded-xl bg-brand-500/10 text-brand-300 ring-1 ring-brand-500/15">
            <BookOpenTextIcon className="h-5 w-5" />
          </div>
          <input
            className="min-w-0 flex-1 rounded-xl border border-white/10 bg-black/40 px-4 text-white outline-none transition-all placeholder:text-white/30 focus:border-brand-500/50 focus:ring-2 focus:ring-brand-500/20"
            placeholder="Add a quick note to this task..."
            value={note}
            onChange={(event) => setNote(event.target.value)}
          />
          <motion.button
            type="submit"
            whileHover={{ y: -1 }}
            whileTap={{ scale: 0.97 }}
            className="btn-primary"
          >
            Add Note
          </motion.button>
        </div>
      </form>
    </div>
  )
}

function Metric({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  return (
    <div
      className={`rounded-xl border p-3 transition-colors ${
        accent ? 'border-brand-500/30 bg-brand-500/[0.06]' : 'border-white/[0.06] bg-white/[0.015]'
      }`}
    >
      <p className="text-[11px] font-medium uppercase tracking-[0.14em] text-white/40">{label}</p>
      <p className={`mt-1 text-sm font-semibold capitalize ${accent ? 'text-brand-200' : 'text-white/85'}`}>{value}</p>
    </div>
  )
}

function duration(task: Task, now: number) {
  if (!task.startedAt) {
    return 'not started'
  }

  const end = task.endedAt ? parseISO(task.endedAt).getTime() : now
  const seconds = Math.max(1, differenceInSeconds(end, parseISO(task.startedAt).getTime()))
  return formatDistanceStrict(0, seconds * 1000)
}
