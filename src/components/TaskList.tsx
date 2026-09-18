import { FormEvent, useEffect, useMemo, useState } from 'react'
import { AnimatePresence } from 'framer-motion'
import { differenceInSeconds, format, formatDistanceStrict, isToday, isYesterday, parseISO } from 'date-fns'
import { CircleCheckIcon, LoaderCircleIcon, LayoutListIcon, PlusIcon, ArrowDownUpIcon, SearchIcon, XIcon } from '@animateicons/react/lucide'
import { useTaskStore } from '../stores/taskStore'
import { SkeletonCard } from './Skeleton'
import TaskFlowLogo from './TaskFlowLogo'
import { HoverCard, modalVariants, motion, selectionSpring, useStaggerContainer, useStaggerItem } from '../lib/motion'
import type { Integration, Task, Ticket } from '../types'

const sourceOptions: Task['source'][] = ['manual', 'jira', 'github', 'linear']

export default function TaskList({ onSelectTimeline }: { onSelectTimeline?: () => void } = {}) {
  const {
    tasks,
    selectedTask,
    events,
    integrations,
    ticketSearchResults,
    isSyncingTickets,
    syncProgress,
    createTask,
    selectTask,
    syncTickets,
    searchTickets,
    createTaskFromTicket,
    setSettingsOpen,
  } = useTaskStore()
  const [modalOpen, setModalOpen] = useState(false)
  const [tab, setTab] = useState<'manual' | 'ticket'>('manual')
  const [title, setTitle] = useState('')
  const [description, setDescription] = useState('')
  const [source, setSource] = useState<Task['source']>('manual')
  const [ticketQuery, setTicketQuery] = useState('')
  const [selectedTicket, setSelectedTicket] = useState<Ticket | null>(null)
  const [branchName, setBranchName] = useState('')
  const [notice, setNotice] = useState<string | null>(null)

  const groupedTasks = useMemo(() => {
    return tasks.reduce<Record<string, Task[]>>((groups, task) => {
      const date = parseISO(task.createdAt)
      const label = isToday(date) ? 'Today' : isYesterday(date) ? 'Yesterday' : format(date, 'MMM d, yyyy')
      groups[label] = [...(groups[label] ?? []), task]
      return groups
    }, {})
  }, [tasks])

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault()
    if (!title.trim()) {
      return
    }

    await createTask(title.trim(), description.trim() || null, source)
    setTitle('')
    setDescription('')
    setSource('manual')
    setModalOpen(false)
  }

  useEffect(() => {
    if (!modalOpen || tab !== 'ticket') {
      return
    }
    const handle = window.setTimeout(() => {
      void searchTickets(ticketQuery.trim())
    }, 300)
    return () => window.clearTimeout(handle)
  }, [modalOpen, searchTickets, tab, ticketQuery])

  const selectTicket = (ticket: Ticket) => {
    setSelectedTicket(ticket)
    setBranchName(ticket.branch ?? '')
  }

  const startFromTicket = async () => {
    if (!selectedTicket) {
      return
    }
    try {
      await createTaskFromTicket(selectedTicket.id, branchName.trim() || selectedTicket.branch)
      setModalOpen(false)
      setSelectedTicket(null)
      setTicketQuery('')
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error))
    }
  }

  const sync = async () => {
    setNotice(null)
    try {
      await syncTickets()
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error))
    }
  }

  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-white/[0.06] p-4">
        <div className="flex items-center justify-between">
          <div>
            <div className="flex items-center gap-1.5 mb-0.5">
              <TaskFlowLogo size="xs" showWordmark={false} />
              <p className="font-display text-[11px] font-medium uppercase tracking-[0.1em] text-white/50">TaskFlow</p>
            </div>
            <h1 className="text-lg font-semibold tracking-tight text-white">Tasks</h1>
          </div>
          <motion.button
            type="button"
            whileHover={{ scale: 1.08, rotate: 90 }}
            whileTap={{ scale: 0.92 }}
            transition={{ type: 'spring', stiffness: 400, damping: 17 }}
            className="flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-br from-brand-500 to-brand-600 text-white shadow-glow-sm"
            onClick={() => setModalOpen(true)}
            aria-label="New task"
          >
            <PlusIcon className="h-[18px] w-[18px]" />
          </motion.button>
        </div>
        <motion.button
          type="button"
          whileTap={{ scale: 0.98 }}
          className="mt-4 flex w-full items-center justify-center gap-2 rounded-xl border border-white/10 bg-white/[0.02] px-3 py-2 text-sm text-white/70 transition-colors hover:border-white/20 hover:text-white disabled:cursor-not-allowed disabled:opacity-50"
          onClick={() => void sync()}
          disabled={isSyncingTickets || integrations.length === 0}
        >
          {isSyncingTickets ? <LoaderCircleIcon className="h-4 w-4 animate-spin text-brand-400" /> : <ArrowDownUpIcon className="h-4 w-4" />}
          {isSyncingTickets ? syncProgress ?? 'Syncing tickets...' : 'Sync tickets'}
        </motion.button>
        {notice && <div className="mt-3 rounded-lg border border-brand-500/30 bg-brand-500/10 p-2 text-xs text-brand-200">{notice}</div>}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {tasks.length === 0 ? (
          <EmptyState onCreate={() => setModalOpen(true)} />
        ) : (
          Object.entries(groupedTasks).map(([date, dateTasks]) => (
            <TaskGroup key={date} label={date}>
              {dateTasks.map((task) => (
                <TaskRow
                  key={task.id}
                  task={task}
                  selected={selectedTask?.id === task.id}
                  eventCount={selectedTask?.id === task.id ? events.length : 0}
                  onClick={() => {
                    void selectTask(task)
                    onSelectTimeline?.()
                  }}
                />
              ))}
            </TaskGroup>
          ))
        )}
      </div>

      <AnimatePresence>
        {modalOpen && (
          <NewTaskModal
            tab={tab}
            setTab={setTab}
            title={title}
            setTitle={setTitle}
            description={description}
            setDescription={setDescription}
            source={source}
            setSource={setSource}
            ticketQuery={ticketQuery}
            setTicketQuery={setTicketQuery}
            integrations={integrations}
            ticketSearchResults={ticketSearchResults}
            selectedTicket={selectedTicket}
            selectTicket={selectTicket}
            branchName={branchName}
            setBranchName={setBranchName}
            onClose={() => setModalOpen(false)}
            onSubmit={handleSubmit}
            onStartFromTicket={startFromTicket}
            setSettingsOpen={setSettingsOpen}
          />
        )}
      </AnimatePresence>
    </div>
  )
}

function EmptyState({ onCreate }: { onCreate: () => void }) {
  return (
    <div className="flex flex-col items-center gap-4 px-4 py-10 text-center">
      <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-white/[0.02] ring-1 ring-white/[0.06]">
        <LayoutListIcon className="h-6 w-6 text-white/30" />
      </div>
      <div>
        <p className="text-sm font-semibold text-white/80">No tasks yet</p>
        <p className="mt-1 text-xs text-white/40">Create your first task to start capturing your workflow.</p>
      </div>
      <motion.button
        type="button"
        whileHover={{ y: -1 }}
        whileTap={{ scale: 0.98 }}
        className="btn-primary"
        onClick={onCreate}
      >
        <PlusIcon className="h-4 w-4" />
        New Task
      </motion.button>
    </div>
  )
}

function TaskGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <section className="mb-5">
      <h2 className="mb-2 px-2 text-[11px] font-semibold uppercase tracking-[0.1em] text-white/35">{label}</h2>
      <div className="space-y-1.5">{children}</div>
    </section>
  )
}

function ProviderBadge({ provider }: { provider: Ticket['provider'] }) {
  const className =
    provider === 'jira'
      ? 'bg-sky-400/15 text-sky-200'
      : provider === 'github'
        ? 'bg-white/10 text-white/80'
        : 'bg-violet-400/15 text-violet-200'
  return <span className={`rounded-md px-1.5 py-0.5 text-[10px] font-semibold uppercase ${className}`}>{provider}</span>
}

export function TaskRow({ task, selected, eventCount, onClick }: { task: Task; selected: boolean; eventCount: number; onClick: () => void }) {
  const duration = getDuration(task)
  const containerV = useStaggerContainer(0.04)
  const itemV = useStaggerItem()

  return (
    <motion.div variants={containerV} initial="hidden" animate="show">
      <HoverCard>
        <motion.button
          type="button"
          variants={itemV}
          whileTap={{ scale: 0.99 }}
          className={`relative w-full overflow-hidden rounded-xl border p-3 text-left transition-colors ${
            selected
              ? 'border-brand-500/40 bg-brand-500/[0.08]'
              : 'border-white/[0.06] bg-white/[0.015] hover:border-white/15 hover:bg-white/[0.04]'
          }`}
          onClick={onClick}
        >
          {/* Shared-layout selection marker — slides between rows */}
          {selected && (
            <motion.div
              layoutId="task-selected-bar"
              transition={selectionSpring}
              className="absolute inset-y-0 left-0 w-[3px] bg-gradient-to-b from-brand-400 to-brand-600"
            />
          )}
          {/* Active task subtle pulse marker */}
          {task.status === 'active' && (
            <span className="absolute right-3 top-3 flex h-2 w-2">
              <span className="absolute inline-flex h-full w-full animate-pulse-ring rounded-full bg-brand-500 opacity-60" />
              <span className="relative inline-flex h-2 w-2 rounded-full bg-brand-500" />
            </span>
          )}
          <div className="mb-2 flex items-start justify-between gap-3 pr-4">
            <h3 className="line-clamp-2 text-sm font-semibold text-white/90">{task.title}</h3>
          </div>
          <div className="flex items-center justify-between text-xs text-white/40">
            <span>{duration}</span>
            <span className="flex items-center gap-1">
              <CircleCheckIcon className="h-1.5 w-1.5 fill-current text-brand-400/60" />
              {eventCount} events
            </span>
          </div>
        </motion.button>
      </HoverCard>
    </motion.div>
  )
}

interface NewTaskModalProps {
  tab: 'manual' | 'ticket'
  setTab: (t: 'manual' | 'ticket') => void
  title: string
  setTitle: (s: string) => void
  description: string
  setDescription: (s: string) => void
  source: Task['source']
  setSource: (s: Task['source']) => void
  ticketQuery: string
  setTicketQuery: (s: string) => void
  integrations: Integration[]
  ticketSearchResults: Ticket[]
  selectedTicket: Ticket | null
  selectTicket: (t: Ticket) => void
  branchName: string
  setBranchName: (s: string) => void
  onClose: () => void
  onSubmit: (e: FormEvent) => void
  onStartFromTicket: () => void
  setSettingsOpen: (open: boolean) => void
}

function NewTaskModal(p: NewTaskModalProps) {
  return (
    <>
      <motion.div
        variants={modalVariants}
        initial="hidden"
        animate="visible"
        exit="exit"
        className="fixed inset-0 z-[80] flex items-center justify-center bg-black/70 p-4 backdrop-blur-sm"
        onClick={p.onClose}
      />
      <motion.form
        variants={modalVariants}
        initial="hidden"
        animate="visible"
        exit="exit"
        className="fixed inset-0 z-[81] flex items-center justify-center p-4"
        onSubmit={p.onSubmit}
      >
        <div
          className="w-full max-w-lg overflow-hidden rounded-2xl border border-white/10 bg-noir-900/95 p-5 shadow-elevated backdrop-blur-xl"
          onClick={(e) => e.stopPropagation()}
        >
          <div className="mb-5 flex items-center justify-between">
            <h2 className="text-lg font-semibold tracking-tight text-white">New Task</h2>
            <button className="rounded-lg p-1 text-white/40 transition-colors hover:bg-white/5 hover:text-white" onClick={p.onClose} type="button">
              <XIcon className="h-5 w-5" />
            </button>
          </div>

          {/* Animated segmented tab control */}
          <div className="relative mb-5 grid grid-cols-2 gap-1 rounded-xl border border-white/[0.06] bg-black/30 p-1">
            {(['manual', 'ticket'] as const).map((t) => (
              <button
                key={t}
                className={`relative z-10 rounded-lg px-3 py-2 text-sm font-semibold capitalize transition-colors ${
                  p.tab === t ? 'text-white' : 'text-white/50 hover:text-white/80'
                }`}
                onClick={() => p.setTab(t)}
                type="button"
              >
                {p.tab === t && (
                  <motion.div
                    layoutId="newtask-tab"
                    transition={selectionSpring}
                    className="absolute inset-0 -z-10 rounded-lg bg-gradient-to-r from-brand-500 to-brand-600 shadow-glow-sm"
                  />
                )}
                {t === 'ticket' ? 'From Ticket' : 'Manual'}
              </button>
            ))}
          </div>

          <AnimatePresence mode="wait">
            {p.tab === 'manual' ? (
              <motion.div
                key="manual"
                initial={{ opacity: 0, x: -8 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: 8 }}
                transition={{ duration: 0.2 }}
              >
                <label className="mb-4 block">
                  <span className="mb-1.5 block text-sm text-white/70">Title</span>
                  <input
                    className="input"
                    value={p.title}
                    onChange={(e) => p.setTitle(e.target.value)}
                    autoFocus
                  />
                </label>
                <label className="mb-4 block">
                  <span className="mb-1.5 block text-sm text-white/70">Description</span>
                  <textarea className="textarea h-24" value={p.description} onChange={(e) => p.setDescription(e.target.value)} />
                </label>
                <label className="mb-5 block">
                  <span className="mb-1.5 block text-sm text-white/70">Source</span>
                  <select className="select" value={p.source} onChange={(e) => p.setSource(e.target.value as Task['source'])}>
                    {sourceOptions.map((option) => (
                      <option key={option} value={option}>
                        {option}
                      </option>
                    ))}
                  </select>
                </label>
                <button className="btn-primary w-full py-2.5" type="submit">
                  Create task
                </button>
              </motion.div>
            ) : (
              <motion.div
                key="ticket"
                initial={{ opacity: 0, x: 8 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: -8 }}
                transition={{ duration: 0.2 }}
              >
                {p.integrations.length === 0 ? (
                  <div className="rounded-xl border border-dashed border-white/15 p-4 text-sm text-white/60">
                    Connect Jira, GitHub, or Linear in{' '}
                    <button
                      className="text-brand-300 hover:text-brand-200"
                      onClick={() => {
                        p.onClose()
                        p.setSettingsOpen(true)
                      }}
                      type="button"
                    >
                      Settings
                    </button>
                  </div>
                ) : (
                  <>
                    <label className="mb-3 block">
                      <span className="mb-1.5 block text-sm text-white/70">Search tickets</span>
                      <div className="flex items-center gap-2 rounded-xl border border-white/10 bg-black/40 px-3 py-2.5 transition-all focus-within:border-brand-500/50 focus-within:ring-2 focus-within:ring-brand-500/20">
                        <SearchIcon className="h-4 w-4 text-white/40" />
                        <input
                          className="min-w-0 flex-1 bg-transparent text-sm text-white outline-none placeholder:text-white/30"
                          placeholder="Search Jira, GitHub, Linear..."
                          value={p.ticketQuery}
                          onChange={(e) => p.setTicketQuery(e.target.value)}
                        />
                      </div>
                    </label>
                    <div className="mb-4 max-h-56 space-y-2 overflow-y-auto">
                      {p.ticketSearchResults.map((ticket) => (
                        <button
                          key={ticket.id}
                          className={`w-full rounded-xl border p-3 text-left text-sm transition-colors ${
                            p.selectedTicket?.id === ticket.id
                              ? 'border-brand-500/40 bg-brand-500/10'
                              : 'border-white/[0.06] bg-white/[0.02] hover:bg-white/[0.05]'
                          }`}
                          onClick={() => p.selectTicket(ticket)}
                          type="button"
                        >
                          <div className="mb-1 flex items-center gap-2">
                            <ProviderBadge provider={ticket.provider} />
                            <span className="font-mono text-xs text-white/50">{ticket.ticketId}</span>
                            {ticket.priority && (
                              <span className="rounded bg-amber-400/15 px-1.5 py-0.5 text-[10px] text-amber-200">{ticket.priority}</span>
                            )}
                          </div>
                          <p className="line-clamp-2 font-semibold text-white/90">{ticket.title}</p>
                          {ticket.project && <p className="mt-1 text-xs text-white/40">{ticket.project}</p>}
                        </button>
                      ))}
                    </div>
                    {p.selectedTicket && (
                      <div className="mb-4 rounded-xl border border-white/[0.06] bg-black/30 p-3 text-sm">
                        <h3 className="mb-1 font-semibold text-white/90">{p.selectedTicket.title}</h3>
                        <p className="mb-2 line-clamp-4 text-white/50">{p.selectedTicket.description?.slice(0, 200) || 'No description provided.'}</p>
                        {p.selectedTicket.labels && <p className="mb-2 text-xs text-white/40">Labels: {p.selectedTicket.labels}</p>}
                        <label className="block">
                          <span className="mb-1 block text-xs text-white/50">Suggested branch name</span>
                          <input
                            className="input"
                            value={p.branchName}
                            onChange={(e) => p.setBranchName(e.target.value)}
                          />
                        </label>
                      </div>
                    )}
                    <button
                      className="btn-primary w-full py-2.5"
                      disabled={!p.selectedTicket}
                      onClick={() => void p.onStartFromTicket()}
                      type="button"
                    >
                      Start from this ticket
                    </button>
                  </>
                )}
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </motion.form>
    </>
  )
}

function getDuration(task: Task) {
  if (!task.startedAt) {
    return 'not started'
  }

  const end = task.endedAt ? parseISO(task.endedAt) : new Date()
  const seconds = Math.max(1, differenceInSeconds(end, parseISO(task.startedAt)))
  return formatDistanceStrict(0, seconds * 1000)
}

// re-export for SkeletonCard consumers if needed
export { SkeletonCard }
