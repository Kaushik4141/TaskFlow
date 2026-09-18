import { useCallback, useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { ProjectCandidate } from '../types'

/**
 * Shared state for the auto-detected project review queue.
 *
 * Both surfaces that ask the user about a detected project — the Settings list
 * and the on-launch prompt — run through this hook, so "yes"/"no"/"rename" mean
 * exactly the same thing in both places and neither can drift from the backend's
 * rules.
 *
 * Approving with a corrected name is deliberately the *same* call as approving
 * as-is, just with `displayName` supplied: the backend renames the row and then
 * approves it, and because rows are keyed by the normalized name the correction
 * behaves as an alias rather than forking a second candidate.
 */
export function useProjectCandidates() {
  const [candidates, setCandidates] = useState<ProjectCandidate[]>([])
  const [loading, setLoading] = useState(true)
  const [busyKey, setBusyKey] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      setCandidates(await invoke<ProjectCandidate[]>('get_project_candidates'))
      setError(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  /** Remove a decided row locally so the list responds without a round trip. */
  const drop = (matchKey: string) =>
    setCandidates((list) => list.filter((item) => item.matchKey !== matchKey))

  /**
   * Confirm a candidate. Pass `displayName` to correct the spelling first.
   * Returns the name that landed in the registry, or null if the call failed.
   */
  const approve = async (matchKey: string, displayName?: string): Promise<string | null> => {
    const candidate = candidates.find((item) => item.matchKey === matchKey)
    const name = displayName?.trim() || candidate?.displayName || matchKey
    setBusyKey(matchKey)
    setError(null)
    try {
      await invoke('approve_project_candidate', {
        matchKey,
        displayName: displayName?.trim() || null,
      })
      drop(matchKey)
      return name
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      return null
    } finally {
      setBusyKey(null)
    }
  }

  /** Reject a candidate. Durable — it is never proposed again. */
  const reject = async (matchKey: string): Promise<boolean> => {
    setBusyKey(matchKey)
    setError(null)
    try {
      await invoke('reject_project_candidate', { matchKey })
      drop(matchKey)
      return true
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      return false
    } finally {
      setBusyKey(null)
    }
  }

  return { candidates, loading, busyKey, error, approve, reject, refresh }
}
