import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { create } from 'zustand'
import type { CaptureStats, Documentation, Event, Integration, Note, Rollup, SearchResult, Task, TaskStats, TestResult, Ticket } from '../types'
import { useToastStore } from './toastStore'

const toast = (type: 'success' | 'error' | 'info', message: string) => {
  useToastStore.getState().addToast(type, message)
}

interface TaskStore {
  tasks: Task[]
  activeTask: Task | null
  selectedTask: Task | null
  events: Event[]
  sidecarReady: boolean
  documentation: Documentation | null
  isGenerating: boolean
  integrations: Integration[]
  tickets: Ticket[]
  ticketSearchResults: Ticket[]
  isSyncingTickets: boolean
  syncProgress: string | null
  settingsOpen: boolean
  captureStats: CaptureStats | null
  searchOpen: boolean
  taskStats: TaskStats | null
  onboardingCompleted: boolean | null
  documentationHistory: Documentation[]
  rollups: Rollup[]
  setSettingsOpen: (settingsOpen: boolean) => void
  setSearchOpen: (searchOpen: boolean) => void
  fetchTasks: () => Promise<void>
  createTask: (title: string, description: string | null, source: Task['source']) => Promise<void>
  startTask: (id: string) => Promise<void>
  stopTask: (id: string) => Promise<void>
  selectTask: (task: Task) => Promise<void>
  selectTaskById: (taskId: string) => Promise<void>
  fetchEvents: (taskId: string) => Promise<void>
  fetchRollups: (taskId: string) => Promise<void>
  addNote: (taskId: string, content: string) => Promise<Note>
  generateDocumentation: (taskId: string) => Promise<Documentation>
  fetchDocumentation: (taskId: string) => Promise<void>
  fetchSidecarStatus: () => Promise<void>
  fetchIntegrations: () => Promise<void>
  saveIntegration: (
    provider: Integration['provider'],
    name: string,
    token: string,
    workspace?: string | null,
    extra?: string | null,
  ) => Promise<Integration>
  deleteIntegration: (id: string) => Promise<void>
  testIntegration: (
    provider: Integration['provider'],
    token: string,
    workspace?: string | null,
    extra?: string | null,
  ) => Promise<TestResult>
  syncTickets: () => Promise<void>
  searchTickets: (query: string, provider?: Integration['provider'] | null) => Promise<void>
  createTaskFromTicket: (ticketId: string, sourceBranch?: string | null) => Promise<void>
  fetchCaptureStats: (taskId: string) => Promise<void>
  updatePrivacySettings: (settings: {
    excludedApps: string[]
    captureClipboard: boolean
    captureScreenText: boolean
    captureWindowTitles: boolean
  }) => Promise<void>
  updateCaptureWorkflow: (settings: {
    mode: 'manual' | 'continuous' | 'selective'
    selectiveApps: string[]
    retentionHours: number
  }) => Promise<void>
  ensureDailyCaptureTask: () => Promise<void>
  fetchTaskStats: () => Promise<void>
  checkOnboarding: () => Promise<void>
  completeOnboarding: () => void
  fetchDocumentationHistory: (taskId: string) => Promise<void>
  searchDocumentation: (query: string) => Promise<SearchResult[]>
}

export const useTaskStore = create<TaskStore>((set, get) => ({
  tasks: [],
  activeTask: null,
  selectedTask: null,
  events: [],
  sidecarReady: false,
  documentation: null,
  isGenerating: false,
  integrations: [],
  tickets: [],
  ticketSearchResults: [],
  isSyncingTickets: false,
  syncProgress: null,
  settingsOpen: false,
  captureStats: null,
  searchOpen: false,
  taskStats: null,
  onboardingCompleted: null,
  documentationHistory: [],
  rollups: [],

  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setSearchOpen: (searchOpen) => set({ searchOpen }),

  fetchTasks: async () => {
    const [tasks, activeTask] = await Promise.all([
      invoke<Task[]>('get_all_tasks'),
      invoke<Task | null>('get_active_task'),
    ])
    const selectedTask = get().selectedTask
    const nextSelected = selectedTask
      ? tasks.find((task) => task.id === selectedTask.id) ?? activeTask ?? tasks[0] ?? null
      : activeTask ?? tasks[0] ?? null

    set({ tasks, activeTask, selectedTask: nextSelected })
    if (nextSelected) {
      await Promise.all([get().fetchEvents(nextSelected.id), get().fetchDocumentation(nextSelected.id), get().fetchRollups(nextSelected.id)])
    }
  },

  createTask: async (title, description, source) => {
    try {
      const task = await invoke<Task>('create_task', { title, description, source })
      set((state) => ({
        tasks: [task, ...state.tasks.map((item) => (item.status === 'active' ? { ...item, status: 'paused' as const } : item))],
        activeTask: task,
        selectedTask: task,
        events: [],
        documentation: null,
      }))
      toast('success', `Task "${title}" created`)
    } catch (err) {
      toast('error', `Failed to create task: ${err}`)
      throw err
    }
  },

  startTask: async (id) => {
    try {
      const task = await invoke<Task>('start_task', { id })
      set((state) => ({
        tasks: state.tasks.map((item) =>
          item.id === task.id ? task : item.status === 'active' ? { ...item, status: 'paused' as const } : item,
        ),
        activeTask: task,
        selectedTask: task,
      }))
      await get().fetchEvents(id)
      toast('success', `Task started — capturing activity`)
    } catch (err) {
      toast('error', `Failed to start task: ${err}`)
      throw err
    }
  },

  stopTask: async (id) => {
    try {
      const task = await invoke<Task>('stop_task', { id })
      set((state) => ({
        tasks: state.tasks.map((item) => (item.id === task.id ? task : item)),
        activeTask: state.activeTask?.id === id ? null : state.activeTask,
        selectedTask: state.selectedTask?.id === id ? task : state.selectedTask,
      }))
      // The backend flushed a final roll-up on stop; refresh the timeline.
      await get().fetchRollups(id)
      toast('info', 'Task stopped')
    } catch (err) {
      toast('error', `Failed to stop task: ${err}`)
      throw err
    }
  },

  selectTask: async (task) => {
    set({ selectedTask: task })
    await Promise.all([get().fetchEvents(task.id), get().fetchDocumentation(task.id), get().fetchRollups(task.id)])
  },

  selectTaskById: async (taskId) => {
    const task = get().tasks.find((t) => t.id === taskId)
    if (task) {
      await get().selectTask(task)
    }
  },

  fetchEvents: async (taskId) => {
    const events = await invoke<Event[]>('get_task_events', { taskId })
    if (get().selectedTask?.id === taskId) {
      set({ events })
    }
  },

  fetchRollups: async (taskId) => {
    const rollups = await invoke<Rollup[]>('get_rollups', { taskId })
    if (get().selectedTask?.id === taskId) {
      set({ rollups })
    }
  },

  addNote: async (taskId, content) => {
    const note = await invoke<Note>('add_note', { taskId, content })
    await get().fetchEvents(taskId)
    return note
  },

  generateDocumentation: async (taskId) => {
    set({ isGenerating: true })
    try {
      const documentation = await invoke<Documentation>('generate_documentation', { taskId })
      if (get().selectedTask?.id === taskId) {
        set({ documentation })
      }
      toast('success', 'Documentation generated')
      return documentation
    } catch (err) {
      toast('error', `Documentation generation failed: ${err}`)
      throw err
    } finally {
      set({ isGenerating: false })
    }
  },

  fetchDocumentation: async (taskId) => {
    const documentation = await invoke<Documentation | null>('get_documentation', { taskId })
    if (get().selectedTask?.id === taskId) {
      set({ documentation })
    }
  },

  fetchSidecarStatus: async () => {
    const sidecarReady = await invoke<boolean>('get_sidecar_status')
    set({ sidecarReady })
  },

  fetchIntegrations: async () => {
    const integrations = await invoke<Integration[]>('get_integrations')
    set({ integrations })
  },

  saveIntegration: async (provider, name, token, workspace = null, extra = null) => {
    const integration = await invoke<Integration>('save_integration', { provider, name, token, workspace, extra })
    set((state) => ({
      integrations: [integration, ...state.integrations.filter((item) => item.id !== integration.id)],
    }))
    toast('success', `${provider} integration saved`)
    return integration
  },

  deleteIntegration: async (id) => {
    await invoke('delete_integration', { id })
    set((state) => ({
      integrations: state.integrations.filter((integration) => integration.id !== id),
      tickets: state.tickets.filter((ticket) => ticket.integrationId !== id),
      ticketSearchResults: state.ticketSearchResults.filter((ticket) => ticket.integrationId !== id),
    }))
    toast('info', 'Integration removed')
  },

  testIntegration: async (provider, token, workspace = null, extra = null) => {
    return invoke<TestResult>('test_integration', { provider, token, workspace, extra })
  },

  syncTickets: async () => {
    set({ isSyncingTickets: true, syncProgress: 'Syncing tickets...' })
    try {
      const tickets = await invoke<Ticket[]>('sync_tickets')
      set({ tickets, ticketSearchResults: tickets.slice(0, 20), syncProgress: `Synced ${tickets.length} tickets` })
      toast('success', `Synced ${tickets.length} tickets`)
    } catch (err) {
      toast('error', `Ticket sync failed: ${err}`)
      throw err
    } finally {
      set({ isSyncingTickets: false })
    }
  },

  searchTickets: async (query, provider = null) => {
    const ticketSearchResults = await invoke<Ticket[]>('search_tickets', { query, provider })
    set({ ticketSearchResults })
  },

  createTaskFromTicket: async (ticketId, sourceBranch = null) => {
    const task = await invoke<Task>('create_task_from_ticket', { ticketId, sourceBranch })
    set((state) => ({
      tasks: [task, ...state.tasks.map((item) => (item.status === 'active' ? { ...item, status: 'paused' as const } : item))],
      activeTask: task,
      selectedTask: task,
      events: [],
      documentation: null,
    }))
    toast('success', `Task created from ticket`)
  },

  fetchCaptureStats: async (taskId) => {
    const captureStats = await invoke<CaptureStats>('get_capture_stats', { taskId })
    if (get().selectedTask?.id === taskId) {
      set({ captureStats })
    }
  },

  updatePrivacySettings: async (settings) => {
    await invoke('update_privacy_settings', settings)
  },

  updateCaptureWorkflow: async (settings) => {
    const task = await invoke<Task | null>('update_capture_workflow', settings)
    if (task) {
      set((state) => ({
        tasks: [task, ...state.tasks.filter((item) => item.id !== task.id).map((item) => (item.status === 'active' ? { ...item, status: 'paused' as const } : item))],
        activeTask: task,
        selectedTask: task,
        events: state.selectedTask?.id === task.id ? state.events : [],
        documentation: state.selectedTask?.id === task.id ? state.documentation : null,
      }))
      await Promise.all([get().fetchEvents(task.id), get().fetchDocumentation(task.id)])
      toast('success', 'Daily memory capture is active')
    } else {
      toast('info', 'Manual task workflow is active')
    }
  },

  ensureDailyCaptureTask: async () => {
    const task = await invoke<Task | null>('ensure_daily_capture_task')
    if (!task) {
      return
    }
    set((state) => ({
      tasks: [task, ...state.tasks.filter((item) => item.id !== task.id).map((item) => (item.status === 'active' ? { ...item, status: 'paused' as const } : item))],
      activeTask: task,
      selectedTask: state.selectedTask ?? task,
    }))
  },

  fetchTaskStats: async () => {
    try {
      const taskStats = await invoke<TaskStats>('get_task_stats')
      set({ taskStats })
    } catch {
      // Stats are non-critical, ignore errors
    }
  },

  checkOnboarding: async () => {
    try {
      const value = await invoke<string | null>('get_setting', { key: 'onboarding_completed' })
      set({ onboardingCompleted: value === 'true' })
    } catch {
      set({ onboardingCompleted: true }) // Assume completed if command not available
    }
  },

  completeOnboarding: () => {
    set({ onboardingCompleted: true })
  },

  fetchDocumentationHistory: async (taskId) => {
    try {
      const documentationHistory = await invoke<Documentation[]>('get_documentation_history', { taskId })
      set({ documentationHistory })
    } catch {
      set({ documentationHistory: [] })
    }
  },

  searchDocumentation: async (query) => {
    return invoke<SearchResult[]>('search_documentation', { query })
  },
}))

let listenersStarted = false

export function startTaskStoreListeners() {
  if (listenersStarted) {
    return
  }
  listenersStarted = true

  const refreshVisibleEvents = () => {
    const { activeTask, selectedTask, fetchEvents } = useTaskStore.getState()
    if (activeTask && selectedTask?.id === activeTask.id) {
      window.setTimeout(() => {
        void fetchEvents(activeTask.id)
      }, 100)
    }
  }

  void listen('window-changed', refreshVisibleEvents)
  void listen('clipboard-changed', refreshVisibleEvents)
  void listen<Rollup>('rollup-created', (event) => {
    // Silently fold the new roll-up into the visible timeline — no toast:
    // roll-ups fire every few minutes and must never demand attention.
    const { selectedTask, fetchRollups } = useTaskStore.getState()
    if (selectedTask && event.payload.taskId === selectedTask.id) {
      void fetchRollups(selectedTask.id)
    }
  })
  void listen<boolean>('sidecar-ready', () => {
    useTaskStore.setState({ sidecarReady: true })
  })
  void listen<string>('ticket-sync-progress', (event) => {
    useTaskStore.setState({ syncProgress: event.payload })
  })
  void useTaskStore.getState().fetchSidecarStatus()
  void useTaskStore.getState().fetchIntegrations()
  void useTaskStore.getState().fetchTaskStats()
}
