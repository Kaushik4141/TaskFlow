import { useCallback, useEffect, useState, useRef } from 'react'
import { AnimatePresence } from 'framer-motion'
import { ActivityIcon, LoaderCircleIcon, PauseIcon, PlayIcon, SearchIcon as SearchIcon, SettingsIcon as SettingsIcon } from '@animateicons/react/lucide'
import ActiveTask from './components/ActiveTask'
import AmbientBackground from './components/AmbientBackground'
import KeyboardShortcutsHelp from './components/KeyboardShortcutsHelp'
import Onboarding from './components/Onboarding'
import ProjectPrompt from './components/ProjectPrompt'
import Search from './components/Search'
import Settings from './components/Settings'
import StatsPanel from './components/StatsPanel'
import TaskList from './components/TaskList'
import Timeline from './components/Timeline'
import ToastContainer from './components/Toast'
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts'
import { motion } from './lib/motion'
import { useViewSwap } from './lib/motion'
import { startTaskStoreListeners, useTaskStore } from './stores/taskStore'
import WindowControls from './components/WindowControls'
import TaskFlowLogo from './components/TaskFlowLogo'

export default function App() {
  const [sidebarWidth, setSidebarWidth] = useState(300)
  const [isResizing, setIsResizing] = useState(false)
  const sidebarRef = useRef<HTMLElement>(null)

  useEffect(() => {
    if (!isResizing) return
    const onMouseMove = (e: MouseEvent) => {
      let newWidth = e.clientX
      if (newWidth < 220) newWidth = 220
      if (newWidth > 600) newWidth = 600
      setSidebarWidth(newWidth)
    }
    const onMouseUp = () => {
      setIsResizing(false)
      document.body.style.cursor = 'default'
    }
    document.body.style.cursor = 'col-resize'
    window.addEventListener('mousemove', onMouseMove)
    window.addEventListener('mouseup', onMouseUp)
    return () => {
      window.removeEventListener('mousemove', onMouseMove)
      window.removeEventListener('mouseup', onMouseUp)
      document.body.style.cursor = 'default'
    }
  }, [isResizing])

  const {
    selectedTask,
    activeTask,
    isGenerating,
    fetchTasks,
    settingsOpen,
    setSettingsOpen,
    searchOpen,
    setSearchOpen,
    taskStats,
    onboardingCompleted,
    checkOnboarding,
    completeOnboarding,
    selectTaskById,
    startTask,
    stopTask,
  } = useTaskStore()

  const [helpOpen, setHelpOpen] = useState(false)
  const [sidebarView, setSidebarView] = useState<'tasks' | 'timeline'>('tasks')
  const viewSwap = useViewSwap()

  const prevTaskIdRef = useRef<string | null>(null)
  useEffect(() => {
    if (selectedTask && selectedTask.id !== prevTaskIdRef.current) {
      setSidebarView('timeline')
      prevTaskIdRef.current = selectedTask.id
    } else if (!selectedTask) {
      setSidebarView('tasks')
      prevTaskIdRef.current = null
    }
  }, [selectedTask])

  useEffect(() => {
    startTaskStoreListeners()
    void fetchTasks()
    void checkOnboarding()
  }, [fetchTasks, checkOnboarding])

  const isCapturing = activeTask !== null || selectedTask?.status === 'active'

  const handleToggleTask = useCallback(() => {
    if (activeTask) {
      void stopTask(activeTask.id)
    } else if (selectedTask?.status === 'active') {
      void stopTask(selectedTask.id)
    } else if (selectedTask) {
      void startTask(selectedTask.id)
    }
  }, [activeTask, selectedTask, startTask, stopTask])

  useKeyboardShortcuts({
    onSearch: () => setSearchOpen(true),
    onSettings: () => setSettingsOpen(!settingsOpen),
    onToggleTask: handleToggleTask,
    onEscape: () => {
      if (searchOpen) setSearchOpen(false)
      else if (helpOpen) setHelpOpen(false)
    },
    onHelp: () => setHelpOpen(true),
  })

  const handleSearchNavigate = useCallback(
    (taskId: string) => {
      setSettingsOpen(false)
      void selectTaskById(taskId)
    },
    [setSettingsOpen, selectTaskById],
  )

  // Show onboarding on first launch
  if (onboardingCompleted === false) {
    return (
      <>
        <Onboarding onComplete={completeOnboarding} />
        <ToastContainer />
      </>
    )
  }

  return (
    <div className="relative h-screen w-screen overflow-hidden rounded-2xl bg-transparent">
      <AmbientBackground />
      <main className="relative z-10 flex h-full flex-col text-white">
        {/* Top-Right Header Region: Two stacked rows */}
        <div className="absolute right-0 top-0 z-[60] flex flex-col items-end pointer-events-none">
          {/* Row 1 (top): WindowControls only — draggable */}
          <div
            data-tauri-drag-region
            className="pointer-events-auto flex h-8 items-center justify-end pr-1"
          >
            <WindowControls className="flex h-full items-center justify-end" />
          </div>

          {/* Row 2 (directly below Row 1): Settings toggle + Stop TaskFlow button — not draggable */}
          <div
            data-tauri-drag-region="false"
            className="pointer-events-auto flex items-center justify-end gap-2 pr-2.5 pt-1"
          >
            <button
              type="button"
              aria-label={settingsOpen ? 'Close settings' : 'Open settings'}
              title={settingsOpen ? 'Close settings' : 'Settings'}
              className={`flex h-8 w-8 items-center justify-center rounded-xl transition-all ${
                settingsOpen
                  ? 'bg-brand-500/20 text-brand-200 ring-1 ring-brand-500/30 shadow-glow-sm'
                  : 'border border-white/10 bg-white/[0.04] text-white/70 hover:bg-white/10 hover:text-white'
              }`}
              onClick={() => setSettingsOpen(!settingsOpen)}
            >
              <SettingsIcon className="h-4 w-4" />
            </button>

            {isCapturing ? (
              <motion.button
                type="button"
                whileHover={{ scale: 1.02 }}
                whileTap={{ scale: 0.98 }}
                disabled={isGenerating}
                onClick={handleToggleTask}
                title="Stop TaskFlow"
                className="inline-flex h-8 items-center gap-2 rounded-xl border border-rose-500/30 bg-rose-500/15 px-3 text-xs font-semibold text-rose-200 transition-all hover:bg-rose-500/25 hover:border-rose-500/40 shadow-sm disabled:cursor-not-allowed disabled:opacity-60"
              >
                {isGenerating ? (
                  <LoaderCircleIcon className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <>
                    <span className="relative flex h-2 w-2">
                      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-rose-400 opacity-75" />
                      <span className="relative inline-flex h-2 w-2 rounded-full bg-rose-500" />
                    </span>
                    <PauseIcon className="h-3.5 w-3.5 text-rose-300" />
                  </>
                )}
                <span>{isGenerating ? 'Generating...' : 'Stop TaskFlow'}</span>
              </motion.button>
            ) : (
              <motion.button
                type="button"
                whileHover={{ scale: selectedTask && !isGenerating ? 1.02 : 1 }}
                whileTap={{ scale: selectedTask && !isGenerating ? 0.98 : 1 }}
                disabled={(!selectedTask && !activeTask) || isGenerating}
                onClick={handleToggleTask}
                title={selectedTask ? 'Start TaskFlow' : 'Select a task to start TaskFlow'}
                className="inline-flex h-8 items-center gap-2 rounded-xl border border-white/10 bg-white/[0.04] px-3 text-xs font-semibold text-white/70 transition-all hover:bg-white/10 hover:text-white disabled:cursor-not-allowed disabled:opacity-40"
              >
                {isGenerating ? (
                  <LoaderCircleIcon className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <PlayIcon className="h-3.5 w-3.5 text-white/60" />
                )}
                <span>{isGenerating ? 'Generating...' : selectedTask ? 'Start TaskFlow' : 'Stop TaskFlow'}</span>
              </motion.button>
            )}
          </div>
        </div>
        {/* Main Content */}
        <div className="relative flex h-full min-h-0 flex-1 p-2 gap-2">
          {/* Sidebar */}
          <aside
            ref={sidebarRef}
            data-tauri-drag-region
            style={{ width: sidebarWidth }}
            className="relative flex shrink-0 flex-col overflow-hidden rounded-xl border border-white/[0.06] bg-gradient-to-b from-brand-500/10 to-noir-900/50 backdrop-blur-md shadow-elevated cursor-grab active:cursor-grabbing"
          >

            
            {/* Sidebar Header */}
            <div className="relative z-10 flex flex-col gap-5 p-5 pt-7">
              <div className="flex items-center gap-2.5">
                <TaskFlowLogo size="md" />
              </div>
              
              <div>
                <button
                  type="button"
                  className="flex w-full items-center justify-between rounded-xl px-3 py-2 text-sm text-white/60 transition-colors hover:bg-white/[0.04] hover:text-white/90"
                  onClick={() => setSearchOpen(true)}
                >
                  <div className="flex items-center gap-2">
                    <SearchIcon className="h-[18px] w-[18px]" />
                    <span>Search</span>
                  </div>
                  <kbd className="rounded-md border border-white/10 bg-white/5 px-1.5 py-0.5 text-[10px] font-medium text-white/40">
                    ⌘K
                  </kbd>
                </button>
              </div>
            </div>
            <div className="relative z-10 min-h-0 flex-1">
              <AnimatePresence mode="wait">
                <motion.div
                  key={settingsOpen ? 'settings' : sidebarView === 'timeline' && selectedTask ? 'timeline' : 'tasks'}
                  className="h-full"
                  {...viewSwap}
                >
                  {settingsOpen ? (
                    <Settings />
                  ) : sidebarView === 'timeline' && selectedTask ? (
                    <Timeline taskId={selectedTask.id} onBack={() => setSidebarView('tasks')} />
                  ) : (
                    <TaskList onSelectTimeline={() => setSidebarView('timeline')} />
                  )}
                </motion.div>
              </AnimatePresence>
            </div>
            <AnimatePresence>
              {!settingsOpen && sidebarView === 'tasks' && (
                <motion.div
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                  className="relative z-10"
                >
                  <StatsPanel stats={taskStats} />
                </motion.div>
              )}
            </AnimatePresence>
          </aside>

          {/* Resizer Handle */}
          <div
            className="relative -ml-2 -mr-2 w-4 cursor-col-resize z-50 group flex items-center justify-center"
            onMouseDown={(e) => {
              e.preventDefault()
              setIsResizing(true)
            }}
          >
            <div className="h-12 w-1 rounded-full bg-white/10 opacity-0 transition-opacity group-hover:opacity-100" />
          </div>

          {/* Main Workspace */}
          <div className="relative min-w-0 flex-1 rounded-xl bg-noir-950/80 overflow-hidden border border-white/[0.04]">
            {/* Drag Region for Main Area */}
            <div data-tauri-drag-region className="absolute left-0 right-48 top-0 z-[40] h-8 cursor-grab active:cursor-grabbing" />
            <AnimatePresence mode="wait">
              {selectedTask ? (
                <motion.div
                  key={`task-${selectedTask.id}`}
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                  transition={{ duration: 0.2 }}
                  className="h-full"
                >
                  <ActiveTask task={selectedTask} />
                </motion.div>
              ) : (
                <motion.div
                  key="empty"
                  initial={{ opacity: 0, scale: 0.98 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0 }}
                  transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
                  className="flex h-full items-center justify-center p-8"
                >
                  <div className="surface max-w-md rounded-2xl p-8 text-center">
                    <div className="relative mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-2xl bg-gradient-to-br from-brand-500/20 to-brand-600/10 ring-1 ring-brand-500/20">
                      <ActivityIcon className="h-6 w-6 text-brand-300" />
                      <div className="absolute inset-0 -z-10 rounded-2xl bg-brand-500/30 blur-xl" />
                    </div>
                    <h1 className="text-balance text-2xl font-semibold tracking-tight text-white">
                      Start documenting work as it happens.
                    </h1>
                    <p className="mt-3 text-sm leading-6 text-white/50">
                      Create a task to begin capturing local window and clipboard activity for your workflow.
                    </p>
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
            </div>
        </div>

        {/* Overlays */}
        <ToastContainer />
        {/* Mounted after onboarding's early return, so a first-run user is never
            asked to confirm projects before they've captured anything. */}
        <ProjectPrompt />
        <AnimatePresence>
          {searchOpen && <Search onClose={() => setSearchOpen(false)} onNavigate={handleSearchNavigate} />}
        </AnimatePresence>
        <AnimatePresence>
          {helpOpen && <KeyboardShortcutsHelp onClose={() => setHelpOpen(false)} />}
        </AnimatePresence>
      </main>
    </div>
  )
}
