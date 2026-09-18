import { useEffect, useState } from 'react'
import type React from 'react'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-shell'
import { UserRoundSearchIcon, CircleCheckIcon, CloudUploadIcon, ExternalLinkIcon, KeyRoundIcon, LayoutListIcon, LoaderCircleIcon, CodeXmlIcon, ShieldCheckIcon, ZapIcon } from '@animateicons/react/lucide'
import type { OllamaStatus, SummarySettings as SummarySettingsType, TestResult } from '../types'

interface CloudModels {
  models: string[]
}

const defaults: SummarySettingsType = {
  mode: 'basic',
  cloudBaseUrl: '',
  cloudApiKey: '',
  cloudModel: '',
  ollamaModel: 'llama3.1:8b',
  ollamaUrl: 'http://localhost:11434',
}

export default function SummarySettings() {
  const [settings, setSettings] = useState<SummarySettingsType>(defaults)
  const [status, setStatus] = useState<OllamaStatus | null>(null)
  const [testResult, setTestResult] = useState<TestResult | null>(null)
  const [saving, setSaving] = useState(false)
  const [checking, setChecking] = useState(false)
  const [testing, setTesting] = useState(false)
  const [saved, setSaved] = useState(false)
  const [cloudModels, setCloudModels] = useState<CloudModels | null>(null)
  const [fetchingModels, setFetchingModels] = useState(false)

  useEffect(() => {
    void invoke<SummarySettingsType>('get_summary_settings').then((loaded) => {
      setSettings({ ...defaults, ...loaded })
    })
  }, [])

  const update = <K extends keyof SummarySettingsType>(key: K, value: SummarySettingsType[K]) => {
    setSaved(false)
    setTestResult(null)
    setSettings((current) => ({ ...current, [key]: value }))
  }

  const checkOllama = async () => {
    setChecking(true)
    setStatus(null)
    try {
      setStatus(await invoke<OllamaStatus>('check_ollama_status', { ollamaUrl: settings.ollamaUrl }))
    } finally {
      setChecking(false)
    }
  }

  const fetchModels = async () => {
    if (!settings.cloudBaseUrl.trim() || !settings.cloudApiKey.trim()) return
    setFetchingModels(true)
    setCloudModels(null)
    try {
      setCloudModels(await invoke<CloudModels>('fetch_cloud_models', { baseUrl: settings.cloudBaseUrl, apiKey: settings.cloudApiKey }))
    } catch (error) {
      setTestResult({ success: false, message: error instanceof Error ? error.message : String(error) })
    } finally {
      setFetchingModels(false)
    }
  }

  const testSettings = async () => {
    setTesting(true)
    setTestResult(null)
    try {
      setTestResult(await invoke<TestResult>('test_summary_settings', { settings }))
    } catch (error) {
      setTestResult({ success: false, message: error instanceof Error ? error.message : String(error) })
    } finally {
      setTesting(false)
    }
  }

  const save = async () => {
    setSaving(true)
    setSaved(false)
    try {
      await invoke('save_summary_settings', { settings })
      setSaved(true)
    } finally {
      setSaving(false)
    }
  }

  return (
    <section>
      <div className="mb-4">
        <h2 className="text-sm font-semibold text-slate-200">Summary Generation</h2>
        <p className="mt-1 text-sm leading-6 text-slate-400">Choose how TaskFlow generates documentation when you complete a task.</p>
      </div>

      <div className="space-y-3">
        <ModePanel checked={settings.mode === 'basic'} icon={<ZapIcon className="h-4 w-4 text-slate-300" />} title="Basic" badge="recommended for most" onSelect={() => update('mode', 'basic')}>
          <p>Instant, 100% local, no setup.</p>
          <p>Uses smart text analysis and works completely offline.</p>
        </ModePanel>

        <ModePanel checked={settings.mode === 'local_ai'} icon={<CodeXmlIcon className="h-4 w-4 text-cyan-300" />} title="Local AI" badge="private, slower" onSelect={() => update('mode', 'local_ai')}>
          <p>Runs an AI model on your machine with Ollama.</p>
          <div className="mt-3 grid gap-3 sm:grid-cols-2">
            <TextField label="Ollama URL" value={settings.ollamaUrl} onChange={(value) => update('ollamaUrl', value)} />
            <TextField label="Model" value={settings.ollamaModel} onChange={(value) => update('ollamaModel', value)} />
          </div>
          <div className="mt-3 flex flex-wrap gap-2">
            <button className="inline-flex items-center gap-2 rounded-md border border-white/10 px-3 py-2 text-sm text-slate-200 hover:bg-white/10 disabled:opacity-60" disabled={checking} onClick={checkOllama} type="button">
              {checking ? <LoaderCircleIcon className="h-4 w-4 animate-spin" /> : <UserRoundSearchIcon className="h-4 w-4" />}
              Check Ollama Status
            </button>
            <button className="inline-flex items-center gap-2 rounded-md border border-white/10 px-3 py-2 text-sm text-slate-200 hover:bg-white/10" onClick={() => void open('https://ollama.ai')} type="button">
              <ExternalLinkIcon className="h-4 w-4" />
              Install Guide
            </button>
          </div>
          {settings.mode === 'local_ai' && <p className="mt-3 rounded-md border border-cyan-300/20 bg-cyan-300/10 p-3 text-xs leading-5 text-cyan-100">Generating with local AI may take a few minutes on first run.</p>}
          {status && (
            <div className="mt-3 rounded-md border border-white/10 bg-black/20 p-3 text-sm">
              <p className={status.isRunning ? 'text-slate-300' : 'text-red-300'}>Status: {status.isRunning ? 'Running' : 'Not running'}</p>
              <p className="mt-1 text-xs text-slate-400">Models: {status.availableModels.length ? status.availableModels.join(', ') : 'none detected'}</p>
              {status.isRunning && <p className={status.hasRecommendedModel ? 'mt-1 text-xs text-slate-300' : 'mt-1 text-xs text-amber-300'}>{status.hasRecommendedModel ? 'Recommended model found.' : 'Install a llama, mistral, or qwen model for best results.'}</p>}
            </div>
          )}
        </ModePanel>

        <ModePanel checked={settings.mode === 'cloud_ai'} icon={<CloudUploadIcon className="h-4 w-4 text-indigo-300" />} title="Cloud AI" badge="best quality" onSelect={() => update('mode', 'cloud_ai')}>
          <p>Uses your own API key. Data goes directly to your provider.</p>
          <p>Supports any OpenAI-compatible endpoint (OpenAI, Nvidia Nim, Together, etc.).</p>
          {settings.mode === 'cloud_ai' && <div className="mt-3 rounded-md border border-amber-300/30 bg-amber-300/10 p-3 text-xs leading-5 text-amber-100">Your captured activity will be sent to the configured endpoint to generate summaries. Nothing else is sent.</div>}
          {settings.mode === 'cloud_ai' && (
            <div className="mt-3 space-y-3">
              <div className="grid gap-3 sm:grid-cols-2">
                <TextField label="Base URL" value={settings.cloudBaseUrl} onChange={(value) => update('cloudBaseUrl', value)} placeholder="https://api.openai.com/v1" />
                <TextField label="API Key" type="password" value={settings.cloudApiKey} onChange={(value) => update('cloudApiKey', value)} />
              </div>
              <div className="flex flex-wrap items-end gap-2">
                <div className="min-w-0 flex-1">
                  <label className="block">
                    <span className="mb-1 block text-sm text-slate-300">Model</span>
                    <select className="w-full rounded-md border border-white/10 bg-black/30 px-3 py-2 text-sm outline-none ring-slate-500 focus:ring-2" value={settings.cloudModel} onChange={(event) => update('cloudModel', event.target.value)}>
                      <option value="">-- Select a model --</option>
                      {cloudModels?.models.map((model) => <option key={model} value={model}>{model}</option>)}
                    </select>
                  </label>
                </div>
                <button className="inline-flex h-10 items-center gap-2 rounded-md border border-white/10 px-3 text-sm text-slate-200 hover:bg-white/10 disabled:opacity-60" disabled={fetchingModels || !settings.cloudBaseUrl.trim() || !settings.cloudApiKey.trim()} onClick={fetchModels} type="button">
                  {fetchingModels ? <LoaderCircleIcon className="h-4 w-4 animate-spin" /> : <LayoutListIcon className="h-4 w-4" />}
                  Fetch Models
                </button>
                <button className="inline-flex h-10 items-center gap-2 rounded-md border border-white/10 px-3 text-sm text-slate-200 hover:bg-white/10 disabled:opacity-60" disabled={testing || !settings.cloudApiKey.trim()} onClick={testSettings} type="button">
                  {testing ? <LoaderCircleIcon className="h-4 w-4 animate-spin" /> : <KeyRoundIcon className="h-4 w-4" />}
                  Test
                </button>
              </div>
            </div>
          )}
        </ModePanel>
      </div>

      {settings.mode !== 'cloud_ai' && <button className="mt-4 inline-flex items-center gap-2 rounded-md border border-white/10 px-3 py-2 text-sm text-slate-200 hover:bg-white/10 disabled:opacity-60" disabled={testing} onClick={testSettings} type="button">{testing ? <LoaderCircleIcon className="h-4 w-4 animate-spin" /> : <CircleCheckIcon className="h-4 w-4" />}Test</button>}
      {testResult && <div className={`mt-4 rounded-md border p-3 text-sm ${testResult.success ? 'border-slate-300/30 bg-slate-300/10 text-slate-100' : 'border-red-300/30 bg-red-300/10 text-red-100'}`}>{testResult.message}</div>}
      <div className="mt-5 flex items-center gap-3">
        <button className="inline-flex items-center gap-2 rounded-md bg-slate-500 px-4 py-2 text-sm font-semibold text-slate-950 hover:bg-slate-400 disabled:opacity-60" disabled={saving} onClick={save} type="button">{saving ? <LoaderCircleIcon className="h-4 w-4 animate-spin" /> : <ShieldCheckIcon className="h-4 w-4" />}Save Settings</button>
        {saved && <span className="text-sm text-slate-300">Saved</span>}
      </div>
    </section>
  )
}

function ModePanel({ checked, icon, title, badge, children, onSelect }: { checked: boolean; icon: React.ReactNode; title: string; badge: string; children: React.ReactNode; onSelect: () => void }) {
  return (
    <label className={`block rounded-lg border p-4 transition ${checked ? 'border-slate-400/60 bg-slate-400/10' : 'border-white/10 bg-white/[0.03] hover:bg-white/[0.05]'}`}>
      <div className="flex items-start gap-3">
        <input className="mt-1 h-4 w-4 accent-slate-400" type="radio" checked={checked} onChange={onSelect} />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">{icon}<span className="font-semibold text-slate-100">{title}</span><span className="text-xs text-slate-400">{badge}</span></div>
          <div className="mt-2 space-y-1 text-sm leading-5 text-slate-400">{children}</div>
        </div>
      </div>
    </label>
  )
}

function TextField({ label, value, onChange, type = 'text', placeholder }: { label: string; value: string; onChange: (value: string) => void; type?: string; placeholder?: string }) {
  return (
    <label className="block">
      <span className="mb-1 block text-sm text-slate-300">{label}</span>
      <input className="w-full rounded-md border border-white/10 bg-black/30 px-3 py-2 text-sm outline-none ring-slate-500 focus:ring-2" type={type} value={value} placeholder={placeholder} onChange={(event) => onChange(event.target.value)} />
    </label>
  )
}
