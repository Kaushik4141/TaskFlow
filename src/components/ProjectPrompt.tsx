import { useEffect, useState } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { SparklesIcon, XIcon } from '@animateicons/react/lucide'
import { CandidateCard } from './CandidateCard'
import { useProjectCandidates } from '../hooks/useProjectCandidates'
import { modalVariants } from '../lib/motion'

/**
 * The most a single launch will ask about. Detection can stage a long tail after
 * a busy week; asking about all of it at once turns a helpful question into a
 * chore, and an unanswered chore gets dismissed wholesale. The rest stay staged
 * and surface in Settings, or on a later launch.
 */
const MAX_PER_LAUNCH = 3

/**
 * Asks, on app open, whether the projects TaskFlow detected are real.
 *
 * Only `ready` candidates are asked about — the ones the backend has corroborated
 * with two independent signals or across two separate days. Everything below that
 * bar stays quiet in the Settings queue rather than interrupting on one sighting.
 *
 * Dismissal is per-launch, not permanent: closing this leaves the candidates
 * staged, so the question returns next time the app opens. That's deliberate —
 * "not now" and "no" are different answers, and only an explicit No is durable
 * (it also vetoes the name from the window-title fallback, so a rejected project
 * can't reappear as a vault page).
 */
export default function ProjectPrompt() {
  const { candidates, loading, busyKey, error, approve, reject } = useProjectCandidates()
  const [dismissed, setDismissed] = useState(false)

  const ready = candidates.filter((candidate) => candidate.ready)
  const asking = ready.slice(0, MAX_PER_LAUNCH)
  const deferred = ready.length - asking.length
  const visible = !dismissed && !loading && asking.length > 0

  // Esc dismisses, matching every other overlay in the app. The effect bails when
  // hidden but is still declared unconditionally to keep hook order stable.
  useEffect(() => {
    if (!visible) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setDismissed(true)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [visible])

  // Backdrop and panel are one element rather than the two that
  // KeyboardShortcutsHelp uses: they share the same variants here, so splitting
  // them would buy nothing and cost AnimatePresence its single keyed child.
  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          key="project-prompt"
          variants={modalVariants}
          initial="hidden"
          animate="visible"
          exit="exit"
          className="fixed inset-0 z-[95] flex items-center justify-center bg-black/70 p-4 backdrop-blur-sm"
          onClick={() => setDismissed(true)}
        >
          <div
            className="w-full max-w-md rounded-2xl border border-white/10 bg-noir-900/95 p-6 shadow-elevated backdrop-blur-xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="mb-4 flex items-start justify-between gap-3">
              <div className="flex items-center gap-3">
                <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-brand-500/10 text-brand-300 ring-1 ring-brand-500/15">
                  <SparklesIcon className="h-[18px] w-[18px]" />
                </div>
                <div>
                  <h2 className="text-lg font-semibold tracking-tight text-white">
                    {asking.length === 1 ? 'Is this a new project?' : 'Are these new projects?'}
                  </h2>
                  <p className="mt-0.5 text-xs leading-4 text-white/45">
                    Spotted in your activity. Confirming adds it to your vault.
                  </p>
                </div>
              </div>
              <button
                className="rounded-lg p-1.5 text-white/40 transition-colors hover:bg-white/5 hover:text-white"
                onClick={() => setDismissed(true)}
                type="button"
                aria-label="Ask me later"
              >
                <XIcon className="h-5 w-5" />
              </button>
            </div>

            {error && (
              <p className="mb-3 rounded-lg bg-red-500/10 p-2 text-xs leading-4 text-red-300/80">{error}</p>
            )}

            <div className="space-y-2">
              {asking.map((candidate) => (
                <CandidateCard
                  key={candidate.matchKey}
                  candidate={candidate}
                  busy={busyKey === candidate.matchKey}
                  size="comfortable"
                  onApprove={(matchKey, displayName) => void approve(matchKey, displayName)}
                  onReject={(matchKey) => void reject(matchKey)}
                />
              ))}
            </div>

            <div className="mt-4 flex items-center justify-between gap-3">
              <span className="text-[11px] text-white/30">
                {deferred > 0 ? `${deferred} more in Settings` : 'Change your mind later in Settings'}
              </span>
              <button type="button" className="btn-ghost text-xs" onClick={() => setDismissed(true)}>
                Ask me later
              </button>
            </div>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  )
}
