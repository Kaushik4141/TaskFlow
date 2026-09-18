import { Dispatch, FormEvent, ReactNode, SetStateAction, useEffect, useMemo, useRef, useState } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { invoke } from '@tauri-apps/api/core'
import { BrainIcon, CircleCheckIcon, BookOpenTextIcon, FolderOpenIcon, GitBranchIcon, KeyRoundIcon, LinkIcon, CodeXmlIcon, ShieldCheckIcon, Trash2Icon, XIcon, ZapIcon, SparklesIcon, EyeIcon, TerminalIcon, SettingsIcon, SunIcon, CopyIcon, CheckIcon, UserIcon } from '@animateicons/react/lucide'
import SummarySettings from './SummarySettings'
import EventFeed from './EventFeed'
import { CandidateCard } from './CandidateCard'
import { useTaskStore } from '../stores/taskStore'
import { useToastStore } from '../stores/toastStore'
import { useProjectCandidates } from '../hooks/useProjectCandidates'
import { selectionSpring } from '../lib/motion'
import type { Integration, LintReport, ProjectCandidate, CloudStatus } from '../types'

const providers: Array<{ id: Integration['provider']; label: string; color: string }> = [
  { id: 'jira', label: 'Jira', color: 'text-sky-300' },
  { id: 'github', label: 'GitHub', color: 'text-white/80' },
  { id: 'linear', label: 'Linear', color: 'text-violet-300' },
]

export default function Settings() {
  const { integrations, deleteIntegration, updatePrivacySettings, updateCaptureWorkflow, selectedTask, documentation } = useTaskStore()
  const [connecting, setConnecting] = useState<Integration['provider'] | null>(null)
  const [showDeepCapture, setShowDeepCapture] = useState(false)
  const [activeSection, setActiveSection] = useState<'capture' | 'integrations' | 'summary' | 'cloud' | 'appearance' | 'developer'>('capture')
  const [privacyLoaded, setPrivacyLoaded] = useState(false)
  const [privacy, setPrivacy] = useState({
    windowTitles: true,
    clipboard: true,
    accessibility: true,
    excludedApps: '1Password\nBitwarden\nSpotify',
  })
  const [workflow, setWorkflow] = useState<{
    mode: 'manual' | 'continuous' | 'selective'
    selectiveApps: string
    retentionHours: number
  }>({
    mode: 'manual',
    selectiveApps: 'Code\nChrome\nSlack',
    retentionHours: 72,
  })
  const [obsidian, setObsidian] = useState({
    enabled: false,
    vaultPath: '',
  })
  const [rollup, setRollup] = useState({
    enabled: true,
    intervalMinutes: 10,
  })
  const [projects, setProjects] = useState('')
  const [ocrFallback, setOcrFallback] = useState(true)
  const [obsidianStatus, setObsidianStatus] = useState<string | null>(null)
  const privacyReady = useRef(false)
  const workflowReady = useRef(false)
  const rollupReady = useRef(false)
  const projectsReady = useRef(false)
  const ocrReady = useRef(false)

  const byProvider = useMemo(() => {
    return providers.map((provider) => ({
      ...provider,
      integration: integrations.find((item) => item.provider === provider.id),
    }))
  }, [integrations])

  useEffect(() => {
    void invoke<Record<string, string>>('get_settings')
      .then((settings) => {
        setPrivacy({
          windowTitles: settings.capture_window_titles !== 'false',
          clipboard: settings.capture_clipboard !== 'false',
          accessibility: settings.capture_screen_text !== 'false',
          excludedApps: settings.excluded_apps || '1Password\nBitwarden\nSpotify',
        })
        const mode = ['manual', 'continuous', 'selective'].includes(settings.capture_workflow)
          ? (settings.capture_workflow as 'manual' | 'continuous' | 'selective')
          : 'manual'
        setWorkflow({
          mode,
          selectiveApps: settings.selective_capture_apps || 'Code\nChrome\nSlack',
          retentionHours: Number(settings.raw_capture_retention_hours || 72),
        })
        setObsidian({
          enabled: settings.obsidian_sync_enabled === 'true',
          vaultPath: settings.obsidian_vault_path || '',
        })
        setRollup({
          enabled: settings.rollup_enabled !== 'false',
          intervalMinutes: Number(settings.rollup_interval_minutes || 10),
        })
        setProjects(settings.wiki_known_projects || '')
        // Mirrors the Rust read in window_monitor.rs: `value != "false"`,
        // absent key means enabled.
        setOcrFallback(settings.capture_ocr_fallback !== 'false')
      })
      .finally(() => setPrivacyLoaded(true))
  }, [])

  useEffect(() => {
    if (!privacyLoaded) {
      return
    }
    if (!privacyReady.current) {
      privacyReady.current = true
      return
    }
    const handle = window.setTimeout(() => {
      void updatePrivacySettings({
        excludedApps: privacy.excludedApps.split('\n').map((value) => value.trim()).filter(Boolean),
        captureClipboard: privacy.clipboard,
        captureScreenText: privacy.accessibility,
        captureWindowTitles: privacy.windowTitles,
      })
    }, 300)
    return () => window.clearTimeout(handle)
  }, [privacy, privacyLoaded, updatePrivacySettings])

  useEffect(() => {
    if (!privacyLoaded) {
      return
    }
    if (!workflowReady.current) {
      workflowReady.current = true
      return
    }
    const handle = window.setTimeout(() => {
      void updateCaptureWorkflow({
        mode: workflow.mode,
        selectiveApps: workflow.selectiveApps.split('\n').map((value) => value.trim()).filter(Boolean),
        retentionHours: workflow.retentionHours,
      })
    }, 300)
    return () => window.clearTimeout(handle)
  }, [privacyLoaded, updateCaptureWorkflow, workflow])

  useEffect(() => {
    if (!privacyLoaded) {
      return
    }
    if (!rollupReady.current) {
      rollupReady.current = true
      return
    }
    const handle = window.setTimeout(() => {
      void invoke('update_setting', { key: 'rollup_enabled', value: String(rollup.enabled) })
      void invoke('update_setting', { key: 'rollup_interval_minutes', value: String(rollup.intervalMinutes) })
    }, 300)
    return () => window.clearTimeout(handle)
  }, [rollup, privacyLoaded])

  useEffect(() => {
    if (!privacyLoaded) {
      return
    }
    if (!projectsReady.current) {
      projectsReady.current = true
      return
    }
    const handle = window.setTimeout(() => {
      const value = projects
        .split('\n')
        .map((name) => name.trim())
        .filter(Boolean)
        .join('\n')
      void invoke('update_setting', { key: 'wiki_known_projects', value })
    }, 300)
    return () => window.clearTimeout(handle)
  }, [projects, privacyLoaded])

  useEffect(() => {
    if (!privacyLoaded) {
      return
    }
    if (!ocrReady.current) {
      ocrReady.current = true
      return
    }
    const handle = window.setTimeout(() => {
      void invoke('update_setting', { key: 'capture_ocr_fallback', value: String(ocrFallback) })
    }, 300)
    return () => window.clearTimeout(handle)
  }, [ocrFallback, privacyLoaded])

  const saveObsidianSettings = async () => {
    setObsidianStatus(null)
    try {
      await invoke('update_obsidian_vault_settings', {
        enabled: obsidian.enabled,
        vaultPath: obsidian.vaultPath,
      })
      setObsidianStatus(obsidian.enabled ? 'Obsidian vault connected.' : 'Obsidian sync disabled.')
    } catch (error) {
      setObsidianStatus(error instanceof Error ? error.message : String(error))
    }
  }

  const syncCurrentToObsidian = async () => {
    if (!selectedTask) {
      setObsidianStatus('Select a generated memory task before syncing.')
      return
    }
    setObsidianStatus(null)
    try {
      const result = await invoke<{ path: string }>('sync_task_to_obsidian', { taskId: selectedTask.id })
      setObsidianStatus(`Synced to ${result.path}`)
    } catch (error) {
      setObsidianStatus(error instanceof Error ? error.message : String(error))
    }
  }

  const tabs = [
    { id: 'capture' as const, icon: <EyeIcon className="h-4 w-4" />, label: 'Capture' },
    { id: 'integrations' as const, icon: <LinkIcon className="h-4 w-4" />, label: 'Integrations' },
    { id: 'summary' as const, icon: <SparklesIcon className="h-4 w-4" />, label: 'Summary' },
    { id: 'cloud' as const, icon: <BrainIcon className="h-4 w-4" />, label: 'Cloud & MCP' },
    { id: 'appearance' as const, icon: <SunIcon className="h-4 w-4" />, label: 'Theme' },
  ]

  return (
    <div className="flex h-full flex-col overflow-hidden" style={{ containerType: 'inline-size' }}>
      {/* Compact header + tab bar */}
      <div className="shrink-0 border-b border-white/[0.06] px-4 pb-3 pt-5">
        <h1 className="mb-3 text-[13px] font-semibold uppercase tracking-[0.12em] text-white/50">Settings</h1>
        <div className="flex gap-1 rounded-lg bg-white/[0.04] p-0.5">
          {tabs.map((tab) => (
            <div key={tab.id} className="group relative flex flex-1">
              <button
                className={`relative z-10 flex w-full items-center justify-center py-2 rounded-md transition-colors ${
                  activeSection === tab.id ? 'text-white' : 'text-white/40 hover:text-white/70'
                }`}
                onClick={() => setActiveSection(tab.id)}
                type="button"
                aria-label={tab.label}
                title={tab.label}
              >
                {activeSection === tab.id && (
                  <motion.div
                    layoutId="settings-tab"
                    transition={selectionSpring}
                    className="absolute inset-0 -z-10 rounded-md bg-white/[0.08]"
                  />
                )}
                {activeSection === tab.id && (
                  <motion.span
                    layoutId="settings-tab-underline"
                    transition={selectionSpring}
                    className="absolute -bottom-0.5 left-2.5 right-2.5 h-0.5 rounded-full bg-brand-400 shadow-glow-sm"
                  />
                )}
                {tab.icon}
              </button>
              {/* Tooltip on hover */}
              <div className="pointer-events-none absolute top-full left-1/2 -translate-x-1/2 z-50 pt-1 opacity-0 scale-95 transition-all duration-150 group-hover:opacity-100 group-hover:scale-100">
                <div className="whitespace-nowrap rounded-md border border-white/10 bg-noir-900/95 px-2 py-0.5 text-[10px] font-medium text-white shadow-elevated backdrop-blur-md">
                  {tab.label}
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Scrollable content area */}
      <div className="flex-1 overflow-y-auto px-4 py-4">
        <AnimatePresence mode="wait">
          {activeSection === 'capture' ? (
            <motion.div
              key="capture"
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -6 }}
              transition={{ duration: 0.15 }}
            >
              <CaptureSection
                workflow={workflow}
                setWorkflow={setWorkflow}
                privacy={privacy}
                setPrivacy={setPrivacy}
                obsidian={obsidian}
                setObsidian={setObsidian}
                rollup={rollup}
                setRollup={setRollup}
                projects={projects}
                setProjects={setProjects}
                ocrFallback={ocrFallback}
                setOcrFallback={setOcrFallback}
                obsidianStatus={obsidianStatus}
                setObsidianStatus={setObsidianStatus}
                saveObsidianSettings={saveObsidianSettings}
                syncCurrentToObsidian={syncCurrentToObsidian}
                setShowDeepCapture={setShowDeepCapture}
                onOpenDeveloper={() => setActiveSection('developer')}
              />
            </motion.div>
          ) : activeSection === 'integrations' ? (
            <motion.div
              key="integrations"
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -6 }}
              transition={{ duration: 0.15 }}
            >
              <IntegrationsSection byProvider={byProvider} onConnect={setConnecting} onDisconnect={(id) => void deleteIntegration(id)} />
            </motion.div>
          ) : activeSection === 'summary' ? (
            <motion.div
              key="summary"
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -6 }}
              transition={{ duration: 0.15 }}
            >
              <SummarySettings />
            </motion.div>
          ) : activeSection === 'cloud' ? (
            <motion.div
              key="cloud"
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -6 }}
              transition={{ duration: 0.15 }}
            >
              <CloudSection />
            </motion.div>
          ) : activeSection === 'appearance' ? (
            <motion.div
              key="appearance"
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -6 }}
              transition={{ duration: 0.15 }}
            >
              <AppearanceSection />
            </motion.div>
          ) : (
            <motion.div
              key="developer"
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -6 }}
              transition={{ duration: 0.15 }}
            >
              <DeveloperSection onBack={() => setActiveSection('capture')} />
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      <AnimatePresence>
        {connecting && <ConnectModal provider={connecting} onClose={() => setConnecting(null)} />}
      </AnimatePresence>
      <AnimatePresence>
        {showDeepCapture && <DeepCaptureModal onClose={() => setShowDeepCapture(false)} />}
      </AnimatePresence>
    </div>
  )
}

/* ---------------------------------------------------------------- */
/* Sections                                                          */
/* ---------------------------------------------------------------- */

function CloudSection() {
  const { addToast } = useToastStore()
  const [status, setStatus] = useState<CloudStatus>({
    configured: false,
    supabaseUrl: 'https://cxarbuqzseembonxgpyw.supabase.co',
    supabaseKey: '',
    autoSync: true,
    lastSync: null,
    lastStats: null,
    authenticated: false,
    userId: null,
    userEmail: null,
    userName: null,
    userAvatar: null,
  })
  const [saving, setSaving] = useState(false)
  const [syncing, setSyncing] = useState(false)
  const [urlInput, setUrlInput] = useState('')
  const [keyInput, setKeyInput] = useState('')
  const [autoSyncInput, setAutoSyncInput] = useState(true)
  const [showKey, setShowKey] = useState(false)
  const [manualUserId, setManualUserId] = useState('')
  const [showManualAuth, setShowManualAuth] = useState(false)
  const [mcpGatewayUrl, setMcpGatewayUrl] = useState('https://taskflow-mcp-brain.onrender.com')
  const [mcpMode, setMcpMode] = useState<'cloud' | 'local'>('cloud')
  const [activeMcpTab, setActiveMcpTab] = useState<'cursor' | 'claude' | 'hermes' | 'rpc'>('cursor')
  const [copied, setCopied] = useState(false)

  const fetchStatus = async () => {
    try {
      const data = await invoke<CloudStatus>('get_cloud_status')
      setStatus(data)
      setUrlInput(data.supabaseUrl || 'https://cxarbuqzseembonxgpyw.supabase.co')
      setKeyInput(data.supabaseKey || '')
      setAutoSyncInput(data.autoSync)
      if (data.mcpGatewayUrl) setMcpGatewayUrl(data.mcpGatewayUrl)
      if (data.userId) setManualUserId(data.userId)
    } catch (err) {
      console.error('Failed to load cloud status:', err)
    }
  }

  useEffect(() => {
    void fetchStatus()
  }, [])

  const handleSaveSettings = async (e: FormEvent) => {
    e.preventDefault()
    setSaving(true)
    try {
      await invoke('save_cloud_settings', {
        supabaseUrl: urlInput.trim(),
        supabaseKey: keyInput.trim(),
        autoSync: autoSyncInput,
        mcpGatewayUrl: mcpGatewayUrl.trim(),
      })
      if (manualUserId.trim() && !status.authenticated) {
        await invoke('update_setting', {
          key: 'supabase_user_id',
          value: manualUserId.trim(),
        })
      }
      await fetchStatus()
      addToast('success', 'Cloud settings saved successfully.')
    } catch (err) {
      addToast('error', `Failed to save: ${err}`)
    } finally {
      setSaving(false)
    }
  }

  const handleSyncNow = async () => {
    setSyncing(true)
    try {
      const res = await invoke<any>('sync_cloud_now')
      await fetchStatus()
      const notes = res?.vault_notes_uploaded ?? 0
      const rollups = res?.rollups_uploaded ?? 0
      addToast('success', `Cloud sync complete! (${rollups} rollups, ${notes} notes mirrored)`)
    } catch (err) {
      addToast('error', `Cloud sync failed: ${err}`)
    } finally {
      setSyncing(false)
    }
  }

  const handleGoogleLogin = async () => {
    try {
      const oauthUrl = await invoke<string>('get_google_oauth_url')
      try {
        const { open } = await import('@tauri-apps/plugin-shell')
        await open(oauthUrl)
      } catch {
        window.open(oauthUrl, '_blank')
      }
      addToast('info', 'Opening browser for Google Sign-In...')

      let attempts = 0
      const interval = setInterval(async () => {
        attempts++
        try {
          const current = await invoke<CloudStatus>('get_cloud_status')
          if (current.authenticated) {
            clearInterval(interval)
            setStatus(current)
            addToast('success', `Signed in as ${current.userName || current.userEmail}!`)
          }
        } catch {
          // ignore
        }
        if (attempts >= 30) clearInterval(interval)
      }, 2000)
    } catch (err) {
      addToast('error', `Login initialization failed: ${err}`)
    }
  }

  const handleSignOut = async () => {
    try {
      await invoke('sign_out_cloud')
      await fetchStatus()
      addToast('info', 'Signed out of TaskFlow Cloud.')
    } catch (err) {
      addToast('error', `Sign out failed: ${err}`)
    }
  }

  const effectiveUserId = status.userId || manualUserId || 'default_user'
  const activeBaseUrl = mcpMode === 'cloud'
    ? (mcpGatewayUrl.trim().replace(/\/+$/, '') || 'https://taskflow-mcp-brain.onrender.com')
    : 'http://localhost:8765'

  const getMcpSnippet = () => {
    if (activeMcpTab === 'cursor') {
      return JSON.stringify(
        {
          mcpServers: {
            taskflow: {
              url: `${activeBaseUrl}/sse?user_id=${effectiveUserId}`,
            },
          },
        },
        null,
        2
      )
    } else if (activeMcpTab === 'claude') {
      if (mcpMode === 'cloud') {
        return JSON.stringify(
          {
            mcpServers: {
              taskflow: {
                command: 'npx',
                args: [
                  '-y',
                  'mcp-remote',
                  `${activeBaseUrl}/sse?user_id=${effectiveUserId}`,
                ],
              },
            },
          },
          null,
          2
        )
      } else {
        return JSON.stringify(
          {
            mcpServers: {
              taskflow: {
                command: 'python3',
                args: [
                  '-m',
                  'sidecar.mcp_server',
                  '--user-id',
                  effectiveUserId,
                ],
              },
            },
          },
          null,
          2
        )
      }
    } else if (activeMcpTab === 'hermes') {
      return `# Zero DB credentials required — connects securely via TaskFlow MCP Gateway:
import json
import urllib.request

def query_taskflow_memory(query: str, project: str = "TaskFlow") -> str:
    payload = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "query_graph_memory",
            "arguments": {
                "query": query,
                "project": project,
                "user_id": "${effectiveUserId}",
            },
        },
    }
    req = urllib.request.Request(
        "${activeBaseUrl}/mcp",
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=15) as resp:
        res = json.loads(resp.read().decode("utf-8"))
        return res["result"]["content"][0]["text"]

# Example:
print(query_taskflow_memory("OAuth token refresh bug"))`
    } else {
      return `# Query your personal memory partition directly via the MCP Gateway (No DB credentials needed):
curl -s -X POST "${activeBaseUrl}/mcp" \\
  -H "Content-Type: application/json" \\
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
      "name": "query_graph_memory",
      "arguments": {
        "query": "TaskFlow",
        "project": "TaskFlow",
        "user_id": "${effectiveUserId}"
      }
    }
  }'`
    }
  }

  const handleCopySnippet = async () => {
    const text = getMcpSnippet() || ''
    try {
      const { writeText } = await import('@tauri-apps/plugin-clipboard-manager')
      await writeText(text)
    } catch {
      await navigator.clipboard.writeText(text)
    }
    setCopied(true)
    addToast('success', 'MCP configuration copied to clipboard!')
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center gap-3">
        <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-brand-500/10 text-brand-300 ring-1 ring-brand-500/20">
          <BrainIcon className="h-4 w-4" />
        </div>
        <div>
          <h2 className="text-sm font-semibold tracking-tight text-white">24/7 Cloud Mirror & Agent Bridge</h2>
          <p className="text-[11px] text-white/50">
            Multi-tenant Supabase memory mirror with isolated database access and personal MCP endpoints.
          </p>
        </div>
      </div>

      {/* 1. User Authentication & Multi-Tenancy */}
      <div className="rounded-xl border border-white/10 bg-white/[0.02] p-4 space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <UserIcon className="h-4 w-4 text-brand-300" />
            <h3 className="text-xs font-semibold uppercase tracking-wider text-white/70">User Identity & Multi-Tenancy</h3>
          </div>
          {status.authenticated ? (
            <span className="inline-flex items-center gap-1 rounded-full bg-emerald-500/15 px-2 py-0.5 text-[10px] font-medium text-emerald-400 border border-emerald-500/20">
              <CheckIcon className="h-3 w-3" /> Verified Account
            </span>
          ) : (
            <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] font-medium text-amber-400 border border-amber-500/20">
              Unauthenticated (Local / Anon Mode)
            </span>
          )}
        </div>

        {status.authenticated ? (
          <div className="flex items-center justify-between rounded-lg border border-white/10 bg-noir-900/60 p-3">
            <div className="flex items-center gap-3">
              {status.userAvatar ? (
                <img src={status.userAvatar} alt="avatar" className="h-10 w-10 rounded-full border border-white/20" />
              ) : (
                <div className="flex h-10 w-10 items-center justify-center rounded-full bg-brand-500/20 text-sm font-semibold text-brand-300 border border-brand-500/30">
                  {status.userName ? status.userName[0].toUpperCase() : 'U'}
                </div>
              )}
              <div>
                <div className="text-xs font-semibold text-white">{status.userName || 'TaskFlow User'}</div>
                <div className="text-[11px] text-white/50">{status.userEmail}</div>
                <div className="mt-1 font-mono text-[10px] text-white/40">User ID: {status.userId}</div>
              </div>
            </div>
            <button
              type="button"
              onClick={handleSignOut}
              className="rounded-lg border border-red-500/20 px-3 py-1.5 text-xs text-red-400 hover:bg-red-500/10 transition-colors"
            >
              Sign Out
            </button>
          </div>
        ) : (
          <div className="space-y-3">
            <p className="text-xs text-white/60">
              Sign in with Google to isolate your activity memory under your personal account. Row Level Security (RLS) ensures only your agents can query your data.
            </p>
            <div className="flex flex-wrap items-center gap-3">
              <button
                type="button"
                onClick={handleGoogleLogin}
                className="flex items-center gap-2.5 rounded-lg border border-white/15 bg-white/10 px-4 py-2 text-xs font-medium text-white hover:bg-white/15 transition-all shadow-sm active:scale-95"
              >
                <svg className="h-4 w-4" viewBox="0 0 24 24">
                  <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"/>
                  <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
                  <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.06H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.94l2.85-2.22.81-.63z"/>
                  <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.06l3.66 2.84c.87-2.6 3.3-4.52 6.16-4.52z"/>
                </svg>
                Sign in with Google
              </button>

              <button
                type="button"
                onClick={() => setShowManualAuth(!showManualAuth)}
                className="text-[11px] text-white/40 hover:text-white/70 underline underline-offset-2"
              >
                {showManualAuth ? 'Hide Manual User ID' : 'Or set User ID manually'}
              </button>
            </div>

            {showManualAuth && (
              <div className="pt-2">
                <label className="text-[11px] text-white/50 block mb-1">Custom User UUID (for headless/testing):</label>
                <input
                  type="text"
                  placeholder="e.g. 11111111-2222-3333-4444-555555555555"
                  value={manualUserId}
                  onChange={(e) => setManualUserId(e.target.value)}
                  className="w-full rounded-lg border border-white/10 bg-noir-950/80 px-3 py-1.5 font-mono text-xs text-white placeholder:text-white/20 focus:border-brand-500/50 focus:outline-none"
                />
              </div>
            )}
          </div>
        )}
      </div>

      {/* 2. 24/7 Cloud Mirror Configuration */}
      <form onSubmit={handleSaveSettings} className="rounded-xl border border-white/10 bg-white/[0.02] p-4 space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <ZapIcon className="h-4 w-4 text-brand-300" />
            <h3 className="text-xs font-semibold uppercase tracking-wider text-white/70">Supabase pgvector Mirror</h3>
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              disabled={syncing}
              onClick={handleSyncNow}
              className="flex items-center gap-1.5 rounded-lg border border-brand-500/30 bg-brand-500/15 px-3 py-1 text-xs font-medium text-brand-300 hover:bg-brand-500/25 transition-all disabled:opacity-50"
            >
              <SparklesIcon className={`h-3.5 w-3.5 ${syncing ? 'animate-spin' : ''}`} />
              {syncing ? 'Syncing...' : 'Sync to Cloud Now'}
            </button>
          </div>
        </div>

        <div className="space-y-3">
          <div>
            <label className="mb-1 block text-[11px] font-medium text-white/70">Supabase Project URL</label>
            <input
              type="text"
              required
              value={urlInput}
              onChange={(e) => setUrlInput(e.target.value)}
              placeholder="https://xyz.supabase.co"
              className="w-full rounded-lg border border-white/10 bg-noir-950/80 px-3 py-2 text-xs text-white placeholder:text-white/20 focus:border-brand-500/50 focus:outline-none"
            />
          </div>

          <div>
            <div className="flex items-center justify-between mb-1">
              <label className="block text-[11px] font-medium text-white/70">Supabase API Key (Publishable / Anon)</label>
              <button
                type="button"
                onClick={() => setShowKey(!showKey)}
                className="text-[10px] text-white/40 hover:text-white/70"
              >
                {showKey ? 'Hide' : 'Show'}
              </button>
            </div>
            <input
              type={showKey ? 'text' : 'password'}
              required
              value={keyInput}
              onChange={(e) => setKeyInput(e.target.value)}
              placeholder="sb_publishable_... or service_role"
              className="w-full rounded-lg border border-white/10 bg-noir-950/80 px-3 py-2 font-mono text-xs text-white placeholder:text-white/20 focus:border-brand-500/50 focus:outline-none"
            />
          </div>

          {/* Set-and-Forget Background Sync Toggle */}
          <div className="flex items-center justify-between rounded-lg border border-white/[0.06] bg-white/[0.01] p-3">
            <div>
              <div className="text-xs font-medium text-white">Continuous Background Sync (Set-and-Forget 24/7)</div>
              <div className="text-[11px] text-white/40">Automatically uploads new 10-minute rollups and graph edges in the background.</div>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={autoSyncInput}
              onClick={() => setAutoSyncInput(!autoSyncInput)}
              className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full transition-colors ${
                autoSyncInput ? 'bg-brand-500' : 'bg-white/20'
              }`}
            >
              <span
                className={`pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow-lg ring-0 transition duration-200 ease-in-out ${
                  autoSyncInput ? 'translate-x-4' : 'translate-x-0.5'
                } mt-0.5`}
              />
            </button>
          </div>

          {/* Sync Stats Banner */}
          {status.lastSync && (
            <div className="rounded-lg border border-white/[0.06] bg-noir-900/40 p-2.5 text-[11px] text-white/60 flex items-center justify-between">
              <span>Last synced: {new Date(status.lastSync).toLocaleTimeString()} ({new Date(status.lastSync).toLocaleDateString()})</span>
              {status.lastStats && (
                <span className="text-emerald-400 font-mono">
                  {status.lastStats.rollups_uploaded ?? 0} rollups | {status.lastStats.vault_notes_uploaded ?? 0} notes
                </span>
              )}
            </div>
          )}

          <button
            type="submit"
            disabled={saving}
            className="rounded-lg border border-white/10 bg-white/5 px-4 py-1.5 text-xs font-medium text-white hover:bg-white/10 transition-colors disabled:opacity-50"
          >
            {saving ? 'Saving...' : 'Save Configuration'}
          </button>
        </div>
      </form>

      {/* 3. Personal MCP Connection (Agent Bridge) */}
      <div className="rounded-xl border border-white/10 bg-white/[0.02] p-4 space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <KeyRoundIcon className="h-4 w-4 text-brand-300" />
            <h3 className="text-xs font-semibold uppercase tracking-wider text-white/70">Personal Agent Bridge (MCP)</h3>
          </div>
          <button
            type="button"
            onClick={handleCopySnippet}
            className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/5 px-2.5 py-1 text-xs text-white hover:bg-white/10 transition-colors"
          >
            {copied ? <CheckIcon className="h-3 w-3 text-emerald-400" /> : <CopyIcon className="h-3 w-3 text-white/70" />}
            {copied ? 'Copied!' : 'Copy Config'}
          </button>
        </div>

        <p className="text-xs text-white/50">
          Connect external AI agents directly to your personal memory partition. All queries are securely authenticated via your User ID without exposing database keys.
        </p>

        {/* Security & Zero Credentials Badge */}
        <div className="flex items-center gap-2 rounded-lg border border-emerald-500/20 bg-emerald-500/10 px-3 py-2 text-[11px] text-emerald-300">
          <CheckIcon className="h-3.5 w-3.5 shrink-0 text-emerald-400" />
          <span>Zero Database Credentials Exposed: AI clients communicate solely through the MCP Gateway with scoped User ID filtering.</span>
        </div>

        {/* Gateway Mode Switcher: Cloud vs Local */}
        <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
          <div className="flex items-center gap-1 rounded-lg border border-white/10 bg-noir-900/60 p-1 text-xs">
            <button
              type="button"
              onClick={() => setMcpMode('cloud')}
              className={`rounded px-2.5 py-1 transition-colors ${
                mcpMode === 'cloud' ? 'bg-brand-500/20 text-brand-300 font-medium' : 'text-white/40 hover:text-white/70'
              }`}
            >
              ☁️ Cloud Gateway (Render 24/7)
            </button>
            <button
              type="button"
              onClick={() => setMcpMode('local')}
              className={`rounded px-2.5 py-1 transition-colors ${
                mcpMode === 'local' ? 'bg-brand-500/20 text-brand-300 font-medium' : 'text-white/40 hover:text-white/70'
              }`}
            >
              💻 Local Gateway (localhost)
            </button>
          </div>

          {mcpMode === 'cloud' && (
            <div className="flex items-center gap-1.5 flex-1 min-w-[240px]">
              <input
                type="text"
                value={mcpGatewayUrl}
                onChange={(e) => setMcpGatewayUrl(e.target.value)}
                placeholder="https://taskflow-mcp-brain.onrender.com"
                className="w-full rounded-lg border border-white/10 bg-noir-950/80 px-2.5 py-1 font-mono text-[11px] text-white placeholder:text-white/20 focus:border-brand-500/50 focus:outline-none"
              />
            </div>
          )}
        </div>

        {/* MCP client switcher tabs */}
        <div className="flex gap-1 border-b border-white/[0.06] pb-2 text-xs">
          {(['cursor', 'claude', 'hermes', 'rpc'] as const).map((tabKey) => (
            <button
              key={tabKey}
              type="button"
              onClick={() => setActiveMcpTab(tabKey)}
              className={`rounded px-2.5 py-1 transition-colors ${
                activeMcpTab === tabKey ? 'bg-white/10 text-white font-medium' : 'text-white/40 hover:text-white/70'
              }`}
            >
              {tabKey === 'cursor'
                ? 'Cursor IDE (Remote SSE)'
                : tabKey === 'claude'
                ? 'Claude Desktop'
                : tabKey === 'hermes'
                ? 'Hermes / Python Agent'
                : 'Direct JSON-RPC'}
            </button>
          ))}
        </div>

        {/* Code Snippet Box */}
        <div className="relative overflow-hidden rounded-lg border border-white/10 bg-noir-950 p-3">
          <pre className="font-mono text-[11px] text-brand-200 overflow-x-auto whitespace-pre leading-relaxed">
            {getMcpSnippet()}
          </pre>
        </div>
      </div>
    </div>
  )
}

function DeveloperSection({ onBack }: { onBack: () => void }) {
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2.5">
          <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-brand-500/10 text-brand-300 ring-1 ring-brand-500/20">
            <SettingsIcon className="h-4 w-4" />
          </div>
          <div>
            <h2 className="text-sm font-semibold tracking-tight text-white">Developer Options</h2>
            <p className="text-[11px] text-white/50">
              Live raw event stream (last 50 events) and window telemetry
            </p>
          </div>
        </div>
        <button
          type="button"
          onClick={onBack}
          className="rounded-lg border border-white/10 px-2.5 py-1 text-xs text-white/60 hover:bg-white/5 hover:text-white transition-colors"
        >
          ← Settings
        </button>
      </div>
      <EventFeed />
    </div>
  )
}

function CaptureSection({
  workflow,
  setWorkflow,
  privacy,
  setPrivacy,
  obsidian,
  setObsidian,
  rollup,
  setRollup,
  projects,
  setProjects,
  ocrFallback,
  setOcrFallback,
  obsidianStatus,
  setObsidianStatus,
  saveObsidianSettings,
  syncCurrentToObsidian,
  setShowDeepCapture,
  onOpenDeveloper,
}: {
  workflow: { mode: 'manual' | 'continuous' | 'selective'; selectiveApps: string; retentionHours: number }
  setWorkflow: Dispatch<SetStateAction<typeof workflow>>
  privacy: { windowTitles: boolean; clipboard: boolean; accessibility: boolean; excludedApps: string }
  setPrivacy: Dispatch<SetStateAction<typeof privacy>>
  obsidian: { enabled: boolean; vaultPath: string }
  setObsidian: Dispatch<SetStateAction<typeof obsidian>>
  rollup: { enabled: boolean; intervalMinutes: number }
  setRollup: Dispatch<SetStateAction<typeof rollup>>
  projects: string
  setProjects: Dispatch<SetStateAction<string>>
  ocrFallback: boolean
  setOcrFallback: Dispatch<SetStateAction<boolean>>
  obsidianStatus: string | null
  setObsidianStatus: Dispatch<SetStateAction<string | null>>
  saveObsidianSettings: () => void
  syncCurrentToObsidian: () => void
  setShowDeepCapture: (show: boolean) => void
  onOpenDeveloper: () => void
}) {
  // Vault health is a transient, on-demand read — it belongs to this panel's
  // lifetime, not to the settings that get persisted.
  const [lint, setLint] = useState<LintReport | null>(null)
  const [linting, setLinting] = useState(false)
  const [lintExpanded, setLintExpanded] = useState(false)

  // Lifted out of the candidate list so the section header can react to it: the
  // count drives the badge and forces the section open when a detection lands.
  const queue = useProjectCandidates()
  const readyCount = queue.candidates.filter((candidate) => candidate.ready).length

  /** Confirming a candidate must also land it in the textarea the user edits. */
  const confirmCandidate = async (matchKey: string, displayName?: string) => {
    const name = await queue.approve(matchKey, displayName)
    if (name) {
      setProjects((current) => appendProject(current, name))
    }
  }

  const checkVaultHealth = async () => {
    setObsidianStatus(null)
    setLint(null)
    setLintExpanded(false)
    setLinting(true)
    try {
      setLint(await invoke<LintReport>('lint_wiki'))
    } catch (error) {
      setObsidianStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setLinting(false)
    }
  }

  return (
    <div className="space-y-5">
      {/* Workflow — compact radio-style selector */}
      <section>
        <SectionLabel>Workflow</SectionLabel>
        <div className="space-y-1">
          <WorkflowOption
            active={workflow.mode === 'manual'}
            icon={<CodeXmlIcon className="h-4 w-4" />}
            title="Manual"
            description="Start and stop tasks yourself"
            onClick={() => setWorkflow((s) => ({ ...s, mode: 'manual' }))}
          />
          <WorkflowOption
            active={workflow.mode === 'continuous'}
            icon={<BrainIcon className="h-4 w-4" />}
            title="All-day memory"
            description="Auto-capture everything"
            onClick={() => setWorkflow((s) => ({ ...s, mode: 'continuous' }))}
          />
          <WorkflowOption
            active={workflow.mode === 'selective'}
            icon={<ShieldCheckIcon className="h-4 w-4" />}
            title="Selective"
            description="Approved apps only"
            onClick={() => setWorkflow((s) => ({ ...s, mode: 'selective' }))}
          />
        </div>
      </section>

      {/* Workstream Roll-ups — collapsible */}
      <CollapsibleSection label="Workstream Roll-ups" defaultOpen={rollup.enabled}>
        <Toggle
          checked={rollup.enabled}
          label="Auto roll-up activity"
          onChange={(v) => setRollup((s) => ({ ...s, enabled: v }))}
        />
        <label className="block">
          <span className="mb-1 block text-xs text-white/50">Roll-up interval</span>
          <select
            className="select text-xs"
            value={rollup.intervalMinutes}
            onChange={(e) => setRollup((s) => ({ ...s, intervalMinutes: Number(e.target.value) }))}
            disabled={!rollup.enabled}
          >
            <option value={5}>Every 5 minutes</option>
            <option value={10}>Every 10 minutes</option>
            <option value={15}>Every 15 minutes</option>
            <option value={30}>Every 30 minutes</option>
          </select>
        </label>
        <p className="text-[11px] leading-4 text-white/35">
          Roll-ups also flush early when you go idle or switch workstreams. Each window lands on its
          workstream node in the vault; the daily note becomes a derived index.
        </p>
      </CollapsibleSection>

      {/* Primary Group: Workspace */}
      <div className="space-y-3 pt-1">
        <div className="flex items-center gap-2">
          <span className="text-[10px] font-semibold uppercase tracking-[0.1em] text-brand-300">Workspace</span>
          <div className="h-px flex-1 bg-white/[0.06]" />
        </div>

        {/* Memory Tree — collapsible (prominent) */}
        <CollapsibleSection label="Memory Tree" defaultOpen={true}>
          <label className="block">
            <span className="mb-1 block text-xs text-white/50">Selective apps (one per line)</span>
            <textarea
              className="textarea h-20 text-xs"
              value={workflow.selectiveApps}
              onChange={(e) => setWorkflow((s) => ({ ...s, selectiveApps: e.target.value }))}
              disabled={workflow.mode !== 'selective'}
            />
          </label>
          <label className="block">
            <span className="mb-1 block text-xs text-white/50">Raw capture retention</span>
            <select
              className="select text-xs"
              value={workflow.retentionHours}
              onChange={(e) => setWorkflow((s) => ({ ...s, retentionHours: Number(e.target.value) }))}
            >
              <option value={24}>24 hours</option>
              <option value={72}>72 hours</option>
              <option value={168}>7 days</option>
            </select>
          </label>
        </CollapsibleSection>

        {/* Known Projects — collapsible (prominent) */}
        <CollapsibleSection
          label="Known Projects"
          defaultOpen={true}
          attention={readyCount}
        >
          <ProjectCandidates
            candidates={queue.candidates}
            busyKey={queue.busyKey}
            error={queue.error}
            onApprove={(matchKey, displayName) => void confirmCandidate(matchKey, displayName)}
            onReject={(matchKey) => void queue.reject(matchKey)}
          />
          <label className="block">
            <span className="mb-1 block text-xs text-white/50">Your projects (one per line)</span>
            <textarea
              className="textarea h-20 text-xs"
              value={projects}
              placeholder={'TaskFlow\nSkillForge\ndatavex3'}
              onChange={(e) => setProjects(e.target.value)}
            />
          </label>
          <p className="text-[11px] leading-4 text-white/35">
            Activity matching one of these names becomes a workstream node in the vault
            (Projects/&lt;name&gt;). Everything else routes to Inbox — without this list, a window-title
            guess creates junk project pages.
          </p>
        </CollapsibleSection>
      </div>

      {/* Secondary Group: More Settings (Obsidian Vault & Privacy) */}
      <div className="pt-2">
        <CollapsibleSection label="More Settings" defaultOpen={false}>
          <div className="space-y-4 pt-1 text-xs text-white/70">
            {/* Obsidian Vault */}
            <div className="rounded-xl border border-white/[0.06] bg-black/20 p-3 space-y-3">
              <span className="text-[11px] font-semibold uppercase tracking-[0.1em] text-white/50">Obsidian Vault</span>
              <Toggle checked={obsidian.enabled} label="Sync to Obsidian" onChange={(v) => setObsidian((s) => ({ ...s, enabled: v }))} />
              <label className="block">
                <span className="mb-1 block text-xs text-white/50">Vault path</span>
                <div className="flex items-center gap-2 rounded-lg border border-white/10 bg-black/30 px-2.5 py-2 transition-all focus-within:border-brand-500/40">
                  <FolderOpenIcon className="h-3.5 w-3.5 shrink-0 text-white/35" />
                  <input
                    className="min-w-0 flex-1 bg-transparent text-xs text-white outline-none placeholder:text-white/25"
                    value={obsidian.vaultPath}
                    placeholder="~/Obsidian/Vault or C:\\Users\\you\\Obsidian Vault"
                    onChange={(e) => setObsidian((s) => ({ ...s, vaultPath: e.target.value }))}
                  />
                </div>
              </label>
              <div className="grid grid-cols-3 gap-1.5">
                <motion.button
                  type="button"
                  whileTap={{ scale: 0.98 }}
                  className="btn-primary text-xs"
                  onClick={() => void saveObsidianSettings()}
                >
                  Save
                </motion.button>
                <motion.button
                  type="button"
                  whileTap={{ scale: 0.98 }}
                  className="btn-ghost text-xs"
                  onClick={() => void syncCurrentToObsidian()}
                >
                  Sync Now
                </motion.button>
                <motion.button
                  type="button"
                  whileTap={{ scale: 0.98 }}
                  className="btn-ghost text-xs disabled:opacity-40"
                  disabled={linting}
                  onClick={() => void checkVaultHealth()}
                >
                  {linting ? 'Checking…' : 'Check Health'}
                </motion.button>
              </div>
              {obsidianStatus && (
                <p className="rounded-lg bg-black/20 p-2 text-[11px] leading-4 text-white/55">{obsidianStatus}</p>
              )}
              {lint && <VaultHealth report={lint} expanded={lintExpanded} onToggle={() => setLintExpanded((v) => !v)} />}
            </div>

            {/* Privacy */}
            <div className="rounded-xl border border-white/[0.06] bg-black/20 p-3 space-y-3">
              <span className="text-[11px] font-semibold uppercase tracking-[0.1em] text-white/50">Privacy</span>
              <Toggle checked={privacy.windowTitles} label="Window titles" onChange={(v) => setPrivacy((s) => ({ ...s, windowTitles: v }))} />
              <Toggle checked={privacy.clipboard} label="Clipboard" onChange={(v) => setPrivacy((s) => ({ ...s, clipboard: v }))} />
              <Toggle
                checked={privacy.accessibility}
                label="Screen text (accessibility)"
                onChange={(v) => {
                  setPrivacy((s) => ({ ...s, accessibility: v }))
                  if (v && localStorage.getItem('taskflow.deepCaptureOnboarded') !== 'true') {
                    setShowDeepCapture(true)
                  }
                }}
              />
              {privacy.accessibility && (
                <div className="ml-3 space-y-1 border-l border-white/[0.08] pl-3">
                  <Toggle
                    checked={ocrFallback}
                    label="Read GPU terminals (OCR fallback)"
                    onChange={setOcrFallback}
                  />
                  <p className="text-[11px] leading-4 text-white/35">
                    For windows that expose no readable text, like Warp or WezTerm. Pixels are read in
                    memory and never written to disk.
                  </p>
                </div>
              )}
              <label className="block">
                <span className="mb-1 block text-xs text-white/50">Excluded apps</span>
                <textarea className="textarea h-16 text-xs" value={privacy.excludedApps} onChange={(e) => setPrivacy((s) => ({ ...s, excludedApps: e.target.value }))} />
              </label>
              <p className="flex items-center gap-1.5 text-[11px] text-white/35">
                <ShieldCheckIcon className="h-3 w-3 text-brand-400" />
                All data stays on your device.
              </p>
            </div>
          </div>
        </CollapsibleSection>
      </div>

      {/* Developer Options Banner in Capture Section */}
      <div className="rounded-xl border border-brand-500/30 bg-gradient-to-r from-brand-500/10 to-brand-500/5 p-3.5 mt-4 shadow-sm">
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2.5">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-brand-500/20 text-brand-300 ring-1 ring-brand-500/30">
              <TerminalIcon className="h-4 w-4" />
            </div>
            <div>
              <h3 className="text-xs font-semibold text-white">Developer Options</h3>
              <p className="text-[11px] text-white/50">Live raw event stream & telemetry</p>
            </div>
          </div>
          <button
            type="button"
            onClick={onOpenDeveloper}
            className="inline-flex items-center gap-1.5 rounded-lg bg-brand-500 px-3 py-1.5 text-xs font-semibold text-white shadow-glow-sm hover:bg-brand-400 transition-all shrink-0"
          >
            <span>Open Feed</span>
            <span>→</span>
          </button>
        </div>
      </div>
    </div>
  )
}

function IntegrationsSection({
  byProvider,
  onConnect,
  onDisconnect,
}: {
  byProvider: Array<{ id: Integration['provider']; label: string; color: string; integration?: Integration }>
  onConnect: (id: Integration['provider']) => void
  onDisconnect: (id: string) => void
}) {
  return (
    <section className="mb-6">
      <h2 className="mb-3 text-sm font-semibold text-white/85">Integrations</h2>
      <div className="space-y-3">
        {byProvider.map(({ id, label, color, integration }) => (
          <div key={id} className="rounded-xl border border-white/[0.06] bg-white/[0.02] p-4">
            <div className="mb-3 flex items-center gap-3">
              <ProviderIcon provider={id} className={`h-5 w-5 ${color}`} />
              <div className="min-w-0 flex-1">
                <p className="text-sm font-semibold text-white/90">{label}</p>
                <p className="truncate text-xs text-white/45">
                  {integration ? `Connected as ${integration.name}` : 'Not connected'}
                </p>
              </div>
              {integration ? (
                <CircleCheckIcon className="h-4 w-4 text-brand-400" />
              ) : (
                <span className="h-2 w-2 rounded-full bg-white/20" />
              )}
            </div>
            {integration ? (
              <button
                className="btn-danger w-full"
                onClick={() => void onDisconnect(integration.id)}
                type="button"
              >
                <Trash2Icon className="h-4 w-4" />
                Disconnect
              </button>
            ) : (
              <button
                className="btn-primary w-full"
                onClick={() => onConnect(id)}
                type="button"
              >
                <LinkIcon className="h-4 w-4" />
                Connect
              </button>
            )}
          </div>
        ))}
      </div>
    </section>
  )
}

/* ---------------------------------------------------------------- */
/* Reusable sub-components                                           */
/* ---------------------------------------------------------------- */

function WorkflowOption({
  active,
  icon,
  title,
  description,
  onClick,
}: {
  active: boolean
  icon: ReactNode
  title: string
  description: string
  onClick: () => void
}) {
  return (
    <motion.button
      type="button"
      whileTap={{ scale: 0.99 }}
      className={`relative flex w-full items-center gap-2.5 rounded-lg px-3 py-2.5 text-left transition-colors ${
        active
          ? 'bg-brand-500/[0.08] text-white'
          : 'text-white/60 hover:bg-white/[0.04] hover:text-white/80'
      }`}
      onClick={onClick}
    >
      {active && (
        <motion.div
          layoutId="workflow-active"
          transition={selectionSpring}
          className="absolute inset-y-1 left-0 w-[2px] rounded-full bg-brand-400"
        />
      )}
      <span className={`shrink-0 ${active ? 'text-brand-300' : 'text-white/35'}`}>{icon}</span>
      <span className="min-w-0 flex-1">
        <span className="block text-[13px] font-medium leading-tight">{title}</span>
        <span className="block text-[11px] leading-tight text-white/35">{description}</span>
      </span>
      {active && (
        <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-brand-400" />
      )}
    </motion.button>
  )
}

/* Reusable section heading */
/**
 * Append an approved project to the registry textarea, matching the backend's
 * dedupe rule (`project_key`: lowercase, alphanumeric only) so confirming
 * "Signal Flow" when "SignalFlow" is already listed doesn't create a duplicate
 * line. The backend is still the source of truth — this keeps the visible
 * textarea in step with the write that already happened.
 */
function appendProject(current: string, name: string): string {
  const key = (value: string) => value.toLowerCase().replace(/[^a-z0-9]/g, '')
  const target = key(name)
  const lines = current
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
  if (lines.some((line) => key(line) === target)) {
    return current
  }
  return [...lines, name].join('\n')
}

/**
 * Review queue for projects TaskFlow detected on its own.
 *
 * Three answers, per the agreed design: confirm, reject, or correct the name.
 * Correcting is not a special case — it renames the row and then confirms, and
 * because the row is keyed by the *normalized* name, the correction acts as an
 * alias so the activity that produced the detection keeps matching.
 *
 * Candidates below the promotion bar are still listed, marked "watching", so
 * nothing is hidden — but they're visually quieter than the ones worth deciding.
 */
function ProjectCandidates({
  candidates,
  busyKey,
  error,
  onApprove,
  onReject,
}: {
  candidates: ProjectCandidate[]
  busyKey: string | null
  error: string | null
  onApprove: (matchKey: string, displayName?: string) => void
  onReject: (matchKey: string) => void
}) {
  // Nothing detected is the normal steady state once everything is confirmed —
  // it deserves no UI at all rather than an empty-state panel.
  if (candidates.length === 0 && !error) {
    return null
  }

  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-1.5">
        <SparklesIcon className="h-3 w-3 text-brand-400" />
        <span className="text-[11px] font-medium text-white/60">
          Detected {candidates.length === 1 ? 'project' : 'projects'}
        </span>
      </div>
      {error && (
        <p className="rounded-lg bg-red-500/10 p-2 text-[11px] leading-4 text-red-300/80">{error}</p>
      )}
      {candidates.map((candidate) => (
        <CandidateCard
          key={candidate.matchKey}
          candidate={candidate}
          busy={busyKey === candidate.matchKey}
          onApprove={onApprove}
          onReject={onReject}
        />
      ))}
    </div>
  )
}

function SectionLabel({ children }: { children: ReactNode }) {
  return (
    <h2 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.1em] text-white/40">{children}</h2>
  )
}

/* Collapsible section wrapper */
function CollapsibleSection({
  label,
  defaultOpen = false,
  attention = 0,
  children,
}: {
  label: string
  defaultOpen?: boolean
  /** Count of items inside that need the user; shows a badge and opens the section. */
  attention?: number
  children: ReactNode
}) {
  const [open, setOpen] = useState(defaultOpen)

  // `defaultOpen` is read once at mount, so a section whose contents arrive
  // asynchronously would stay shut exactly when it has something to say —
  // detected projects land after the initial render, and the Known Projects
  // section starts collapsed for anyone with an empty registry. Opening only on
  // the 0 → n edge means a user who closes it again isn't fought every render.
  const wasFlagged = useRef(false)
  useEffect(() => {
    if (attention > 0 && !wasFlagged.current) {
      setOpen(true)
    }
    wasFlagged.current = attention > 0
  }, [attention])

  return (
    <section>
      <button
        type="button"
        className="flex w-full items-center justify-between py-1 text-left"
        onClick={() => setOpen(!open)}
      >
        <span className="flex items-center gap-1.5">
          <span className="text-[11px] font-semibold uppercase tracking-[0.1em] text-white/40">{label}</span>
          {attention > 0 && (
            <span className="rounded-full bg-brand-500/20 px-1.5 text-[10px] font-semibold leading-4 text-brand-300">
              {attention}
            </span>
          )}
        </span>
        <motion.span
          animate={{ rotate: open ? 180 : 0 }}
          transition={{ duration: 0.2 }}
          className="text-white/30"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6"/></svg>
        </motion.span>
      </button>
      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            className="overflow-hidden"
          >
            <div className="space-y-2.5 rounded-lg border border-white/[0.05] bg-white/[0.015] p-3 mt-1.5">
              {children}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </section>
  )
}

function Toggle({ checked, label, onChange }: { checked: boolean; label: string; onChange: (checked: boolean) => void }) {
  return (
    <label className="flex items-center justify-between gap-2 text-xs text-white/60">
      <span>{label}</span>
      <motion.div
        className={`relative h-5 w-9 shrink-0 cursor-pointer rounded-full transition-colors ${checked ? 'bg-brand-500' : 'bg-white/15'}`}
        onClick={() => onChange(!checked)}
      >
        <motion.div
          layout
          transition={{ type: 'spring', stiffness: 500, damping: 32 }}
          className={`absolute top-0.5 h-4 w-4 rounded-full bg-white shadow-md ${checked ? 'right-0.5' : 'left-0.5'}`}
        />
      </motion.div>
    </label>
  )
}

/**
 * Vault health, told as a sentence first. The verdict line is the whole answer
 * when the graph is clean; the details only unfold when there is something to
 * act on. No counts-as-dashboard, no modal — the report is read in place.
 */
function VaultHealth({
  report,
  expanded,
  onToggle,
}: {
  report: LintReport
  expanded: boolean
  onToggle: () => void
}) {
  const orphans = report.orphanPages.length
  const dangling = report.danglingLinks.length
  const gaps = report.hubPagesMissingBacklinks.length
  const issues = orphans + dangling + gaps
  const clean = issues === 0

  const verdict = clean
    ? `${report.totalPages} pages, ${report.totalLinks} links, nothing to fix.`
    : [
        orphans > 0 ? `${orphans} page${orphans === 1 ? '' : 's'} nothing links to` : null,
        dangling > 0 ? `${dangling} link${dangling === 1 ? '' : 's'} pointing nowhere` : null,
        gaps > 0 ? `${gaps} hub${gaps === 1 ? '' : 's'} missing backlinks` : null,
      ]
        .filter(Boolean)
        .join(', ')

  return (
    <div className="rounded-lg border border-white/[0.05] bg-black/20 p-2">
      <button
        type="button"
        className="flex w-full items-start gap-2 text-left"
        onClick={onToggle}
        disabled={clean}
      >
        {clean ? (
          <CircleCheckIcon className="mt-px h-3.5 w-3.5 shrink-0 text-emerald-300/80" />
        ) : (
          <ShieldCheckIcon className="mt-px h-3.5 w-3.5 shrink-0 text-amber-300/80" />
        )}
        <span className="min-w-0 flex-1 text-[11px] leading-4 text-white/60">
          {clean ? 'Vault graph is healthy — ' : 'Vault graph has loose ends — '}
          <span className="text-white/45">{verdict}</span>
        </span>
        {!clean && (
          <motion.span animate={{ rotate: expanded ? 180 : 0 }} transition={{ duration: 0.2 }} className="mt-px text-white/30">
            <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6"/></svg>
          </motion.span>
        )}
      </button>
      <AnimatePresence>
        {expanded && !clean && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
            className="overflow-hidden"
          >
            <div className="mt-2 space-y-2 border-t border-white/[0.06] pt-2">
              <LintGroup label="Nothing links here" items={report.orphanPages} />
              <LintGroup
                label="Links pointing nowhere"
                items={report.danglingLinks.map((link) => `${link.source} → ${link.target}`)}
              />
              <LintGroup
                label="Hubs missing backlinks"
                items={report.hubPagesMissingBacklinks.map(
                  (gap) => `${gap.hubPage} → ${gap.missingBacklinks.join(', ')}`,
                )}
              />
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}

/** One lint category. Long lists are trimmed — this is a nudge, not an audit log. */
function LintGroup({ label, items }: { label: string; items: string[] }) {
  const LIMIT = 8
  if (items.length === 0) return null
  const shown = items.slice(0, LIMIT)
  const rest = items.length - shown.length
  return (
    <div>
      <p className="text-[10px] font-semibold uppercase tracking-[0.1em] text-white/35">{label}</p>
      <ul className="mt-1 space-y-0.5">
        {shown.map((item) => (
          <li key={item} className="truncate font-mono text-[10px] leading-4 text-white/50" title={item}>
            {item}
          </li>
        ))}
      </ul>
      {rest > 0 && <p className="mt-0.5 text-[10px] text-white/30">and {rest} more</p>}
    </div>
  )
}

/* ---------------------------------------------------------------- */
/* Modals                                                            */
/* ---------------------------------------------------------------- */

function DeepCaptureModal({ onClose }: { onClose: () => void }) {
  const [dontShow, setDontShow] = useState(false)
  const close = () => {
    if (dontShow) {
      localStorage.setItem('taskflow.deepCaptureOnboarded', 'true')
    }
    onClose()
  }

  return (
    <>
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        className="fixed inset-0 z-[80] bg-black/70 backdrop-blur-sm"
        onClick={close}
      />
      <motion.div
        initial={{ opacity: 0, scale: 0.96, y: 8 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.97, y: 4 }}
        transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
        className="fixed inset-0 z-[81] flex items-center justify-center p-4"
        onClick={close}
      >
        <div
          className="w-full max-w-md rounded-2xl border border-white/10 bg-noir-900/95 p-5 shadow-elevated backdrop-blur-xl"
          onClick={(e) => e.stopPropagation()}
        >
          <h2 className="mb-3 text-lg font-semibold tracking-tight text-white">Enable Deep Capture</h2>
          <p className="mb-4 text-sm leading-6 text-white/60">
            To generate detailed documentation, TaskFlow needs accessibility access to read window content.
          </p>
          <p className="mb-4 rounded-xl border border-white/[0.06] bg-black/30 p-3 text-sm text-white/50">
            No additional permissions needed on Windows. Some elevated or protected apps may still block capture.
          </p>
          <label className="mb-4 flex items-center gap-2 text-sm text-white/60">
            <input className="h-4 w-4 accent-brand-500" type="checkbox" checked={dontShow} onChange={(e) => setDontShow(e.target.checked)} />
            Don't show again
          </label>
          <button className="btn-primary w-full py-2.5" onClick={close} type="button">
            Got it
          </button>
        </div>
      </motion.div>
    </>
  )
}

function ConnectModal({ provider, onClose }: { provider: Integration['provider']; onClose: () => void }) {
  const { testIntegration, saveIntegration } = useTaskStore()
  const [token, setToken] = useState('')
  const [workspace, setWorkspace] = useState('')
  const [email, setEmail] = useState('')
  const [status, setStatus] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const label = providers.find((item) => item.id === provider)?.label ?? provider

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    setSaving(true)
    setStatus(null)
    try {
      const result = await testIntegration(provider, token, workspace || null, email || null)
      setStatus(result.message)
      if (!result.success) {
        return
      }
      const name = result.message.replace(/^Connected as\s+/i, '')
      await saveIntegration(provider, name, token, workspace || null, email || null)
      onClose()
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setSaving(false)
    }
  }

  return (
    <>
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        className="fixed inset-0 z-[80] bg-black/70 backdrop-blur-sm"
        onClick={onClose}
      />
      <motion.form
        initial={{ opacity: 0, scale: 0.96, y: 8 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.97, y: 4 }}
        transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
        className="fixed inset-0 z-[81] flex items-center justify-center p-4"
        onSubmit={submit}
      >
        <div
          className="w-full max-w-md rounded-2xl border border-white/10 bg-noir-900/95 p-5 shadow-elevated backdrop-blur-xl"
          onClick={(e) => e.stopPropagation()}
        >
          <div className="mb-5 flex items-center justify-between">
            <h2 className="flex items-center gap-2 text-lg font-semibold tracking-tight text-white">
              <ProviderIcon provider={provider} className="h-5 w-5" />
              Connect {label}
            </h2>
            <button className="rounded-lg p-1 text-white/40 transition-colors hover:bg-white/5 hover:text-white" onClick={onClose} type="button">
              <XIcon className="h-5 w-5" />
            </button>
          </div>

          {provider === 'jira' && (
            <>
              <TextField label="Jira URL" value={workspace} onChange={setWorkspace} placeholder="https://company.atlassian.net" />
              <TextField label="Email" value={email} onChange={setEmail} />
            </>
          )}

          <TextField
            label={provider === 'linear' ? 'API Key' : provider === 'github' ? 'Personal Access Token' : 'API Token'}
            type="password"
            value={token}
            onChange={setToken}
          />

          {provider === 'github' && <p className="mb-3 text-xs text-white/40">Required scopes: repo, read:user</p>}

          <a className="mb-4 inline-flex items-center gap-2 text-sm text-brand-300 transition-colors hover:text-brand-200" href={helpUrl(provider)} target="_blank" rel="noreferrer">
            <KeyRoundIcon className="h-4 w-4" />
            {helpLabel(provider)}
          </a>

          {status && <div className="mb-4 rounded-xl border border-white/[0.06] bg-black/30 p-3 text-sm text-white/70">{status}</div>}

          <button
            className="btn-primary w-full py-2.5"
            disabled={saving || !token.trim() || (provider === 'jira' && (!workspace.trim() || !email.trim()))}
            type="submit"
          >
            <ZapIcon className="h-4 w-4" />
            {saving ? 'Testing...' : 'Test & Save'}
          </button>
        </div>
      </motion.form>
    </>
  )
}

function TextField({
  label,
  value,
  onChange,
  placeholder,
  type = 'text',
}: {
  label: string
  value: string
  onChange: (value: string) => void
  placeholder?: string
  type?: string
}) {
  return (
    <label className="mb-4 block">
      <span className="mb-1.5 block text-sm text-white/65">{label}</span>
      <input className="input" type={type} value={value} placeholder={placeholder} onChange={(e) => onChange(e.target.value)} />
    </label>
  )
}

function ProviderIcon({ provider, className }: { provider: Integration['provider']; className?: string }) {
  if (provider === 'github') {
    return <GitBranchIcon className={className} />
  }
  return <span className={`inline-flex items-center justify-center rounded bg-current/10 font-semibold ${className}`}>{provider[0].toUpperCase()}</span>
}

function helpUrl(provider: Integration['provider']) {
  if (provider === 'jira') return 'https://id.atlassian.com/manage-profile/security/api-tokens'
  if (provider === 'github') return 'https://github.com/settings/tokens'
  return 'https://linear.app/settings/api'
}

function helpLabel(provider: Integration['provider']) {
  if (provider === 'jira') return 'How to get Jira API token'
  if (provider === 'github') return 'Create token on GitHub'
  return 'Get Linear API key'
}

function AppearanceSection() {
  const [currentTheme, setCurrentTheme] = useState(() => localStorage.getItem('taskflow.theme') || 'default')

  const themes = [
    { id: 'default', name: 'Emerald (Default)', class: 'bg-[#48ae6b]' },
    { id: 'blue', name: 'Ocean Blue', class: 'bg-[#3b82f6]' },
    { id: 'purple', name: 'Amethyst', class: 'bg-[#a855f7]' },
    { id: 'orange', name: 'Sunset', class: 'bg-[#f97316]' },
    { id: 'red', name: 'Crimson', class: 'bg-[#ef4444]' },
    { id: 'teal', name: 'Teal', class: 'bg-[#14b8a6]' },
    { id: 'cyan', name: 'Cyan', class: 'bg-[#06b6d4]' },
    { id: 'black', name: 'Monochrome', class: 'bg-[#71717a]' },
  ]

  const selectTheme = (themeId: string) => {
    setCurrentTheme(themeId)
    localStorage.setItem('taskflow.theme', themeId)
    document.documentElement.className = themeId === 'default' ? '' : `theme-${themeId}`
  }

  return (
    <section>
      <SectionLabel>Theme Color</SectionLabel>
      <p className="mb-3 text-xs text-white/45">Accent color for gradients and UI.</p>
      <div className="grid grid-cols-4 gap-3">
        {themes.map((theme) => (
          <button
            key={theme.id}
            className="group flex flex-col items-center gap-1.5"
            onClick={() => selectTheme(theme.id)}
            type="button"
          >
            <div
              className={`flex h-8 w-8 items-center justify-center rounded-full transition-all ${theme.class} ${
                currentTheme === theme.id ? 'ring-2 ring-white/70 ring-offset-1 ring-offset-black scale-110' : 'opacity-70 hover:opacity-100 hover:scale-105'
              }`}
            >
              {currentTheme === theme.id && <CircleCheckIcon className="h-3.5 w-3.5 text-white" />}
            </div>
            <span className={`text-[10px] font-medium leading-tight text-center ${currentTheme === theme.id ? 'text-white' : 'text-white/40'}`}>
              {theme.name}
            </span>
          </button>
        ))}
      </div>
    </section>
  )
}
