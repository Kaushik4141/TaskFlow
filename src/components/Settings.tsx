import { Dispatch, FormEvent, ReactNode, SetStateAction, useEffect, useMemo, useRef, useState } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { invoke } from '@tauri-apps/api/core'
import { BrainIcon, CircleCheckIcon, BookOpenTextIcon, FolderOpenIcon, GitBranchIcon, KeyRoundIcon, LinkIcon, CodeXmlIcon, ShieldCheckIcon, Trash2Icon, XIcon, ZapIcon, SparklesIcon, EyeIcon, TerminalIcon, SettingsIcon, SunIcon } from '@animateicons/react/lucide'
import SummarySettings from './SummarySettings'
import EventFeed from './EventFeed'
import { CandidateCard } from './CandidateCard'
import { useTaskStore } from '../stores/taskStore'
import { useProjectCandidates } from '../hooks/useProjectCandidates'
import { selectionSpring } from '../lib/motion'
import type { Integration, LintReport, ProjectCandidate } from '../types'

const providers: Array<{ id: Integration['provider']; label: string; color: string }> = [
  { id: 'jira', label: 'Jira', color: 'text-sky-300' },
  { id: 'github', label: 'GitHub', color: 'text-white/80' },
  { id: 'linear', label: 'Linear', color: 'text-violet-300' },
]

export default function Settings() {
  const { integrations, deleteIntegration, updatePrivacySettings, updateCaptureWorkflow, selectedTask, documentation } = useTaskStore()
  const [connecting, setConnecting] = useState<Integration['provider'] | null>(null)
  const [showDeepCapture, setShowDeepCapture] = useState(false)
  const [activeSection, setActiveSection] = useState<'capture' | 'integrations' | 'summary' | 'appearance' | 'developer'>('capture')
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

      {/* Memory Tree — collapsible */}
      <CollapsibleSection label="Memory Tree" defaultOpen={workflow.mode === 'selective'}>
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

      {/* Known Projects — collapsible */}
      <CollapsibleSection
        label="Known Projects"
        defaultOpen={projects.trim().length > 0}
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

      {/* Obsidian Vault — collapsible */}
      <CollapsibleSection label="Obsidian Vault" defaultOpen={obsidian.enabled}>
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
      </CollapsibleSection>

      {/* Privacy — collapsible */}
      <CollapsibleSection label="Privacy">
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
        {/* OCR is a strict sub-mode of screen text: window_monitor.rs gates the
            whole deep-capture path on capture_screen_text, so this control is a
            no-op when the parent is off. Nested, and hidden when inapplicable. */}
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
      </CollapsibleSection>

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
  return <span className={`inline-flex items-center justify-center rounded bg-current/10 font-bold ${className}`}>{provider[0].toUpperCase()}</span>
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
