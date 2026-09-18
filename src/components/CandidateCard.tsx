import { useState } from 'react'
import { motion } from 'framer-motion'
import type { ProjectCandidate } from '../types'

/**
 * One auto-detected project with the three answers: confirm, reject, or correct
 * the name.
 *
 * Shared by the Settings review queue and the on-launch prompt so the two can't
 * drift — correcting is not a special case in either, it renames the row and
 * then confirms it. Because rows are keyed by the *normalized* name, the
 * correction acts as an alias, so the activity that produced the detection keeps
 * matching afterwards.
 *
 * The rename draft is local state rather than lifted: it is per-card scratch
 * input with no meaning until submitted.
 */
export function CandidateCard({
  candidate,
  busy,
  size = 'compact',
  onApprove,
  onReject,
}: {
  candidate: ProjectCandidate
  busy: boolean
  /** `compact` for the Settings sidebar, `comfortable` for the launch dialog. */
  size?: 'compact' | 'comfortable'
  onApprove: (matchKey: string, displayName?: string) => void
  onReject: (matchKey: string) => void
}) {
  const [editing, setEditing] = useState(false)
  const [draftName, setDraftName] = useState(candidate.displayName)
  const roomy = size === 'comfortable'

  const nameClass = roomy ? 'text-sm' : 'text-xs'
  const bodyClass = roomy ? 'text-xs' : 'text-[11px]'
  const buttonClass = roomy ? 'text-xs' : 'text-[11px]'

  return (
    <div
      className={`rounded-lg border transition-colors ${roomy ? 'p-3.5' : 'p-2.5'} ${
        candidate.ready
          ? 'border-brand-500/25 bg-brand-500/[0.06]'
          : 'border-white/[0.07] bg-black/20'
      }`}
    >
      {editing ? (
        <form
          onSubmit={(e) => {
            e.preventDefault()
            const name = draftName.trim()
            if (name) {
              onApprove(candidate.matchKey, name)
              setEditing(false)
            }
          }}
          className="space-y-1.5"
        >
          <input
            autoFocus
            className={`w-full rounded-md border border-brand-500/40 bg-black/40 px-2 py-1.5 text-white outline-none ${nameClass}`}
            value={draftName}
            onChange={(e) => setDraftName(e.target.value)}
            placeholder="Correct name"
          />
          <div className="flex gap-1.5">
            <button type="submit" className={`btn-primary flex-1 ${buttonClass}`} disabled={!draftName.trim()}>
              Save &amp; add
            </button>
            <button
              type="button"
              className={`btn-ghost ${buttonClass}`}
              onClick={() => {
                setDraftName(candidate.displayName)
                setEditing(false)
              }}
            >
              Cancel
            </button>
          </div>
        </form>
      ) : (
        <>
          <div className="flex items-baseline justify-between gap-2">
            <span className={`truncate font-medium text-white/85 ${nameClass}`}>{candidate.displayName}</span>
            {!candidate.ready && (
              <span className="shrink-0 text-[10px] uppercase tracking-wider text-white/30">watching</span>
            )}
          </div>
          <p className={`mt-0.5 leading-4 text-white/40 ${bodyClass}`}>{describeCandidate(candidate)}</p>
          {candidate.evidence.length > 0 && (
            <p className="mt-1 truncate text-[10px] leading-4 text-white/25" title={candidate.evidence[0]}>
              {candidate.evidence[0]}
            </p>
          )}
          <div className={`flex gap-1.5 ${roomy ? 'mt-2.5' : 'mt-1.5'}`}>
            <motion.button
              type="button"
              whileTap={{ scale: 0.98 }}
              className={`btn-primary flex-1 disabled:opacity-40 ${buttonClass}`}
              disabled={busy}
              onClick={() => onApprove(candidate.matchKey)}
            >
              Yes
            </motion.button>
            <motion.button
              type="button"
              whileTap={{ scale: 0.98 }}
              className={`btn-ghost disabled:opacity-40 ${buttonClass}`}
              disabled={busy}
              onClick={() => onReject(candidate.matchKey)}
            >
              No
            </motion.button>
            <motion.button
              type="button"
              whileTap={{ scale: 0.98 }}
              className={`btn-ghost disabled:opacity-40 ${buttonClass}`}
              disabled={busy}
              onClick={() => setEditing(true)}
            >
              Rename
            </motion.button>
          </div>
        </>
      )}
    </div>
  )
}

/** One line of plain-language provenance: what saw this, and over how long. */
export function describeCandidate(candidate: ProjectCandidate): string {
  const parts: string[] = []
  if (candidate.signals.length > 0) {
    parts.push(candidate.signals.join(' + '))
  }
  parts.push(candidate.dayCount === 1 ? 'seen today' : `seen on ${candidate.dayCount} days`)
  return parts.join(' · ')
}
