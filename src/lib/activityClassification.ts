import type { ActivityPrediction } from '../types'

const ACTIVITY_LABELS: Record<string, string> = {
  coding_debugging: 'Coding & debugging',
  research_learning: 'Research & learning',
  writing_documentation: 'Writing & documentation',
  communication: 'Communication',
  planning_administration: 'Planning & administration',
  entertainment_media: 'Entertainment & media',
  system_navigation: 'System navigation',
  other: 'Other activity',
}

const ACTIVITY_UNCLEAR = 'Activity unclear'

/** Unknown model categories never leak internal IDs or imply an Other prediction. */
export function activityCategoryLabel(category: unknown): string {
  return typeof category === 'string' && Object.prototype.hasOwnProperty.call(ACTIVITY_LABELS, category)
    ? ACTIVITY_LABELS[category]
    : ACTIVITY_UNCLEAR
}

/** Legacy events remain quiet; uncertainty is conveyed by words, not color. */
export function activityPredictionText(activity?: ActivityPrediction | null): string | null {
  if (activity == null) return null
  if (typeof activity !== 'object') return ACTIVITY_UNCLEAR
  const label = activityCategoryLabel(activity.category)
  if (label === ACTIVITY_UNCLEAR) return ACTIVITY_UNCLEAR
  if (activity.uncertain === false) return `Predicted activity: ${label}`
  if (activity.uncertain !== true) return ACTIVITY_UNCLEAR

  // v1 replaced an abstained candidate with Other; its original winner is unavailable.
  if (activity.model_version === 'synthetic-bootstrap-v1') {
    return `${ACTIVITY_UNCLEAR} · insufficient evidence`
  }
  if (activity.category === 'other'
    || typeof activity.model_version !== 'string'
    || !activity.model_version.trim()) return ACTIVITY_UNCLEAR

  return `${ACTIVITY_UNCLEAR} · possible ${label}`
}
