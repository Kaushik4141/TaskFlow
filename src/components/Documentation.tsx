import { downloadDir, join } from '@tauri-apps/api/path'
import { writeTextFile } from '@tauri-apps/plugin-fs'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { invoke } from '@tauri-apps/api/core'
import { AnimatePresence, motion } from 'framer-motion'
import { ActivityIcon, BookOpenIcon, CheckIcon, ChevronDownIcon, ClipboardIcon, DownloadIcon, BookOpenTextIcon, LinkIcon, ArrowDownUpIcon, SparklesIcon } from '@animateicons/react/lucide'
import { useEffect, useMemo, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import type { Documentation as DocumentationRecord } from '../types'
import { useTaskStore } from '../stores/taskStore'
import { SkeletonDocument } from './Skeleton'
import { useStaggerContainer, useStaggerItem } from '../lib/motion'

export default function Documentation({ documentation }: { documentation: DocumentationRecord }) {
  const { isGenerating, generateDocumentation, selectedTask } = useTaskStore()
  const [copied, setCopied] = useState(false)
  const [exported, setExported] = useState(false)
  const [historyOpen, setHistoryOpen] = useState(false)
  const [history, setHistory] = useState<DocumentationRecord[]>([])
  const [selectedVersion, setSelectedVersion] = useState<DocumentationRecord>(documentation)
  const keyPoints = useMemo(() => extractSectionBullets(selectedVersion.content, 'Key Points'), [selectedVersion.content])
  const resources = useMemo(() => extractSectionBullets(selectedVersion.content, 'Resources Referenced'), [selectedVersion.content])
  const timeline = useMemo(() => extractTimeline(selectedVersion.content), [selectedVersion.content])
  const bodyMarkdown = useMemo(() => stripExtractedSections(selectedVersion.content), [selectedVersion.content])

  useEffect(() => {
    setSelectedVersion(documentation)
  }, [documentation])

  const loadHistory = async () => {
    if (historyOpen) {
      setHistoryOpen(false)
      return
    }
    try {
      const docs = await invoke<DocumentationRecord[]>('get_documentation_history', { taskId: documentation.taskId })
      setHistory(docs)
      setHistoryOpen(true)
    } catch (err) {
      console.error('Failed to load documentation history:', err)
    }
  }

  const copyMarkdown = async () => {
    await writeText(selectedVersion.content)
    setCopied(true)
    window.setTimeout(() => setCopied(false), 1600)
  }

  const exportMarkdown = async () => {
    const dir = await downloadDir()
    const fileName = `taskflow-${selectedVersion.taskId}-v${selectedVersion.version}.md`
    await writeTextFile(await join(dir, fileName), selectedVersion.content)
    setExported(true)
    window.setTimeout(() => setExported(false), 1600)
  }

  const handleRegenerate = async () => {
    if (!selectedTask || isGenerating) return
    await generateDocumentation(selectedTask.id)
  }

  const containerV = useStaggerContainer(0.05)
  const itemV = useStaggerItem()

  if (isGenerating) {
    return <SkeletonDocument />
  }

  return (
    <article className="h-full overflow-y-auto p-6">
      <div className="mx-auto max-w-4xl space-y-5">
        {/* Header */}
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-white/40">Documentation</p>
            <div className="mt-1 flex flex-wrap items-center gap-2">
              <h2 className="text-2xl font-semibold tracking-tight text-white">Generated notes</h2>
              <span className="rounded-full border border-white/[0.08] bg-white/[0.02] px-2 py-0.5 text-xs font-medium text-white/50">
                v{selectedVersion.version}
              </span>
              {selectedVersion.aiMode && (
                <span className="inline-flex items-center rounded-full border border-brand-500/30 bg-brand-500/10 px-2 py-0.5 text-xs font-medium text-brand-200">
                  <SparklesIcon className="mr-1 h-3 w-3" />
                  {selectedVersion.aiMode}
                </span>
              )}
            </div>
            <p className="mt-1 text-xs text-white/35">{new Date(selectedVersion.generatedAt).toLocaleString()}</p>
          </div>
          <div className="flex flex-wrap gap-2">
            <ActionButton onClick={() => void loadHistory()} icon={<ActivityIcon className="h-4 w-4" />} label="History">
              <ChevronDownIcon className={`h-3 w-3 transition-transform ${historyOpen ? 'rotate-180' : ''}`} />
            </ActionButton>
            <ActionButton onClick={() => void handleRegenerate()} disabled={isGenerating} icon={<ArrowDownUpIcon className={`h-4 w-4 ${isGenerating ? 'animate-spin' : ''}`} />} label="Regenerate" />
            <ActionButton onClick={() => void copyMarkdown()} icon={copied ? <CheckIcon className="h-4 w-4 text-brand-300" /> : <ClipboardIcon className="h-4 w-4" />} label={copied ? 'Copied' : 'Copy'} />
            <button
              className="inline-flex items-center gap-2 rounded-xl border border-white/15 bg-white/[0.08] px-3 py-2 text-sm font-semibold text-white shadow-elevated transition-all hover:border-white/25 hover:bg-white/[0.12]"
              onClick={() => void exportMarkdown()}
              type="button"
            >
              {exported ? <CheckIcon className="h-4 w-4" /> : <DownloadIcon className="h-4 w-4" />}
              {exported ? 'Exported' : 'Export .md'}
            </button>
          </div>
        </div>

        {/* Version History Dropdown */}
        <AnimatePresence>
          {historyOpen && history.length > 0 && (
            <motion.div
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: 'auto', opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              transition={{ duration: 0.25, ease: [0.22, 1, 0.36, 1] }}
              className="overflow-hidden"
            >
              <div className="rounded-xl border border-white/[0.06] bg-white/[0.02] p-3">
                <p className="mb-2 px-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-white/35">Version History</p>
                <div className="max-h-56 space-y-1 overflow-y-auto">
                  {history.map((doc) => (
                    <button
                      key={doc.id}
                      className={`flex w-full items-center justify-between rounded-lg px-3 py-2 text-left text-sm transition-colors ${
                        selectedVersion.id === doc.id ? 'bg-brand-500/10 text-brand-200' : 'text-white/70 hover:bg-white/[0.04]'
                      }`}
                      onClick={() => {
                        setSelectedVersion(doc)
                        setHistoryOpen(false)
                      }}
                      type="button"
                    >
                      <span className="font-semibold">Version {doc.version}</span>
                      <span className="text-xs text-white/40">{new Date(doc.generatedAt).toLocaleString()}</span>
                    </button>
                  ))}
                </div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        <motion.div variants={containerV} initial="hidden" animate="show" className="space-y-5">
          {/* Summary highlight card */}
          {selectedVersion.summary && (
            <motion.section
              variants={itemV}
              className="relative overflow-hidden rounded-2xl border border-brand-500/20 bg-gradient-to-br from-brand-500/[0.08] via-brand-500/[0.03] to-transparent p-5"
            >
              <div className="absolute -right-4 -top-4 h-24 w-24 rounded-full bg-brand-500/15 blur-2xl" />
              <div className="relative flex items-start gap-3">
                <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-brand-500/20 text-brand-200 ring-1 ring-brand-500/20">
                  <BookOpenTextIcon className="h-4 w-4" />
                </div>
                <div className="min-w-0">
                  <p className="mb-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-brand-300/80">Summary</p>
                  <p className="text-sm leading-6 text-white/90">{selectedVersion.summary}</p>
                </div>
              </div>
            </motion.section>
          )}

          <div className="grid gap-4 sm:grid-cols-2">
            {/* Key Points card */}
            {keyPoints.length > 0 && (
              <motion.section variants={itemV} className="rounded-2xl border border-white/[0.06] bg-white/[0.02] p-5">
                <div className="mb-3 flex items-center gap-2">
                  <BookOpenIcon className="h-4 w-4 text-brand-400" />
                  <h3 className="text-sm font-semibold uppercase tracking-[0.14em] text-white/65">Key points</h3>
                </div>
                <ul className="space-y-2.5 text-sm text-white/80">
                  {keyPoints.map((point) => (
                    <li className="flex gap-2 leading-6" key={point}>
                      <span className="mt-2 h-1.5 w-1.5 shrink-0 rounded-full bg-brand-400" />
                      <span>{point}</span>
                    </li>
                  ))}
                </ul>
              </motion.section>
            )}

            {/* Resources card */}
            {resources.length > 0 && (
              <motion.section variants={itemV} className="rounded-2xl border border-white/[0.06] bg-white/[0.02] p-5">
                <div className="mb-3 flex items-center gap-2">
                  <LinkIcon className="h-4 w-4 text-brand-400" />
                  <h3 className="text-sm font-semibold uppercase tracking-[0.14em] text-white/65">Resources</h3>
                </div>
                <ul className="space-y-2 text-sm">
                  {resources.map((resource) => (
                    <li className="leading-6" key={resource}>
                      <a className="inline-flex items-center gap-1.5 break-all text-brand-300 transition-colors hover:text-brand-200" href={resource} rel="noreferrer" target="_blank">
                        <LinkIcon className="h-3 w-3 shrink-0" />
                        {resource}
                      </a>
                    </li>
                  ))}
                </ul>
              </motion.section>
            )}
          </div>

          {/* Activity Timeline card */}
          {timeline.length > 0 && (
            <motion.section variants={itemV} className="rounded-2xl border border-white/[0.06] bg-white/[0.02] p-5">
              <div className="mb-4 flex items-center gap-2">
                <ActivityIcon className="h-4 w-4 text-brand-400" />
                <h3 className="text-sm font-semibold uppercase tracking-[0.14em] text-white/65">Activity timeline</h3>
              </div>
              <ol className="relative space-y-3 border-l border-brand-500/20 pl-4">
                {timeline.map((item, idx) => (
                  <li key={idx} className="relative">
                    <span className="absolute -left-[21px] top-1.5 h-2 w-2 rounded-full border-2 border-noir-900 bg-brand-500" />
                    <div className="text-sm text-white/80">{item.text}</div>
                    {item.meta && <p className="mt-0.5 text-xs text-white/40">{item.meta}</p>}
                  </li>
                ))}
              </ol>
            </motion.section>
          )}

          {/* Remaining markdown body (title, description, body) */}
          {bodyMarkdown.trim() && (
            <motion.section
              variants={itemV}
              className="markdown-body max-w-none rounded-2xl border border-white/[0.04] bg-white/[0.015] p-6"
            >
              <ReactMarkdown>{bodyMarkdown}</ReactMarkdown>
            </motion.section>
          )}
        </motion.div>
      </div>
    </article>
  )
}

function ActionButton({
  onClick,
  icon,
  label,
  disabled,
  children,
}: {
  onClick: () => void
  icon: React.ReactNode
  label: string
  disabled?: boolean
  children?: React.ReactNode
}) {
  return (
    <motion.button
      type="button"
      whileHover={{ y: -1 }}
      whileTap={{ scale: 0.97 }}
      className="inline-flex items-center gap-2 rounded-xl border border-white/10 bg-white/[0.02] px-3 py-2 text-sm font-semibold text-white/85 transition-colors hover:bg-white/[0.06] disabled:opacity-50"
      onClick={onClick}
      disabled={disabled}
    >
      {icon}
      {label}
      {children}
    </motion.button>
  )
}

function extractSectionBullets(markdown: string, heading: string) {
  const lines = markdown.split('\n')
  const start = lines.findIndex((line) => line.trim() === `## ${heading}`)
  if (start === -1) {
    return []
  }
  const bullets: string[] = []
  for (const line of lines.slice(start + 1)) {
    if (line.startsWith('## ')) {
      break
    }
    if (line.startsWith('- ')) {
      bullets.push(line.slice(2).trim())
    }
  }
  return bullets.filter((item) => item && !item.toLowerCase().startsWith('no '))
}

function findSectionRange(markdown: string, heading: string): [number, number] | null {
  const lines = markdown.split('\n')
  const start = lines.findIndex((line) => line.trim() === `## ${heading}`)
  if (start === -1) return null
  let end = lines.length
  for (let i = start + 1; i < lines.length; i++) {
    if (lines[i].startsWith('## ')) {
      end = i
      break
    }
  }
  return [start, end]
}

function stripExtractedSections(markdown: string) {
  const lines = markdown.split('\n')
  const dropSet = new Set<number>()

  if (lines.length && lines[0].startsWith('# ')) {
    dropSet.add(0)
    for (let i = 1; i < lines.length; i++) {
      const trimmed = lines[i].trim()
      if (trimmed === '' || trimmed.startsWith('**')) {
        dropSet.add(i)
      } else {
        break
      }
    }
  }

  for (const heading of ['Summary', 'Key Points', 'Resources Referenced', 'Activity Timeline']) {
    const range = findSectionRange(markdown, heading)
    if (!range) continue
    const [start, end] = range
    for (let i = start; i <= end && i < lines.length; i++) dropSet.add(i)
    if (end + 1 < lines.length && lines[end + 1].trim() === '') dropSet.add(end + 1)
  }

  const result = lines.filter((_, i) => !dropSet.has(i)).join('\n')
  return result.replace(/\n{3,}/g, '\n\n').trim()
}

function extractTimeline(markdown: string): Array<{ text: string; meta?: string }> {
  const lines = markdown.split('\n')
  const start = lines.findIndex((line) => line.trim() === '## Activity Timeline')
  if (start === -1) return []
  const items: Array<{ text: string; meta?: string }> = []
  for (const line of lines.slice(start + 1)) {
    if (line.startsWith('## ')) break
    const match = line.match(/^-\s+\*\*(.+?)\*\*\s+`([^`]+)`:?\s*(.*)$/)
    if (match) {
      const [, app, timestamp, detail] = match
      items.push({
        text: detail || app,
        meta: `${app} \u00b7 ${timestamp}`,
      })
    } else if (line.startsWith('- ')) {
      items.push({ text: line.slice(2).trim() })
    }
  }
  return items
}
