export interface Task {
  id: string
  title: string
  description: string | null
  source: 'manual' | 'jira' | 'github' | 'linear' | 'notion' | 'memory'
  sourceId: string | null
  sourceUrl: string | null
  sourceTitle: string | null
  sourceBody: string | null
  sourceLabels: string | null
  sourceAssignee: string | null
  sourcePriority: string | null
  sourceProject: string | null
  sourceBranch: string | null
  status: 'active' | 'paused' | 'completed'
  startedAt: string | null
  endedAt: string | null
  createdAt: string
}

export interface Event {
  id: string
  taskId: string
  eventType: 'window_switch' | 'clipboard' | 'note' | 'url'
  appName: string | null
  windowTitle: string | null
  content: string | null
  url: string | null
  contentType: string | null
  captureMethod: string | null
  isSanitized: number
  chunkIndex: number
  relevance: number
  timestamp: string
  createdAt: string
}

export interface Note {
  id: string
  taskId: string
  content: string
  appName: string | null
  timestamp: string
}

export interface Documentation {
  id: string
  taskId: string
  content: string
  summary: string | null
  version: number
  generatedAt: string
  aiMode: string | null
}

export interface Rollup {
  id: string
  taskId: string
  windowStart: string
  windowEnd: string
  title: string
  summaryMd: string
  /** JSON-encoded string array */
  keyPoints: string | null
  /** JSON-encoded string array */
  apps: string | null
  /** JSON-encoded string array */
  resources: string | null
  workstreamSlug: string | null
  aiMode: string | null
  eventCount: number
  triggerKind: 'interval' | 'idle' | 'context_switch' | 'stop' | 'manual' | null
  createdAt: string
}

export interface Integration {
  id: string
  provider: 'jira' | 'github' | 'linear'
  name: string
  workspace: string | null
  createdAt: string
}

export interface Ticket {
  id: string
  integrationId: string | null
  provider: 'jira' | 'github' | 'linear'
  ticketId: string
  title: string
  description: string | null
  labels: string | null
  priority: string | null
  assignee: string | null
  project: string | null
  url: string | null
  branch: string | null
  fetchedAt: string
}

export interface TestResult {
  success: boolean
  message: string
}

export interface SummarySettings {
  mode: 'basic' | 'local_ai' | 'cloud_ai'
  cloudBaseUrl: string
  cloudApiKey: string
  cloudModel: string
  ollamaModel: string
  ollamaUrl: string
}

export interface OllamaStatus {
  isRunning: boolean
  availableModels: string[]
  hasRecommendedModel: boolean
}

export interface CaptureStats {
  totalEvents: number
  withContent: number
  withUrl: number
  titleOnly: number
  excluded: number
  byApp: Record<string, number>
}

/** A `[[wikilink]]` whose target page does not exist in the vault. */
export interface DanglingLink {
  source: string
  target: string
}

/** A hub page missing backlinks to daily notes that already link to it. */
export interface HubPageBacklinkGap {
  hubPage: string
  missingBacklinks: string[]
}

/** Mechanical health report for the TaskFlow-owned section of the vault. */
export interface LintReport {
  vaultPath: string
  totalPages: number
  totalLinks: number
  orphanPages: string[]
  danglingLinks: DanglingLink[]
  hubPagesMissingBacklinks: HubPageBacklinkGap[]
}

/**
 * A project TaskFlow detected on its own, staged for the user to confirm.
 *
 * `matchKey` is the normalized name (lowercase, alphanumeric only) and is the
 * row's identity — correcting the spelling keeps the same key, so a rename is an
 * alias rather than a second candidate.
 */
export interface ProjectCandidate {
  matchKey: string
  displayName: string
  /** Human labels, e.g. `["Repository URL", "Editor workspace"]`. */
  signals: string[]
  /** Up to five captures that produced this candidate. */
  evidence: string[]
  dayCount: number
  eventCount: number
  firstSeen: string
  lastSeen: string
  /** True once ≥2 signal kinds or ≥2 days back the name — worth prompting for. */
  ready: boolean
}

export interface SearchResult {  taskId: string
  taskTitle: string
  matchedSnippet: string
  relevanceScore: number
  createdAt: string
  source: string
}

export interface TaskStats {
  totalTasks: number
  totalTimeSecs: number
  tasksThisWeek: number
  timeThisWeekSecs: number
  mostUsedApps: [string, number][]
  tasksBySource: Record<string, number>
}

export interface CloudStatus {
  configured: boolean
  supabaseUrl: string
  supabaseKey: string
  autoSync: boolean
  lastSync: string | null
  lastStats: {
    vault_notes_uploaded?: number
    rollups_uploaded?: number
    graph_edges_uploaded?: number
    errors?: string[]
    success?: boolean
  } | null
  authenticated: boolean
  userId: string | null
  userEmail: string | null
  userName: string | null
  userAvatar: string | null
  mcpGatewayUrl?: string | null
}

