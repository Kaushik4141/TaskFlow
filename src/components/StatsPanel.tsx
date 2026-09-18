import { useState } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { ChartColumnIcon, ChevronDownIcon, ActivityIcon, BookOpenTextIcon, LayersIcon } from '@animateicons/react/lucide'
import type { TaskStats } from '../types'

export default function StatsPanel({ stats }: { stats: TaskStats | null }) {
  const [expanded, setExpanded] = useState(false)

  if (!stats || stats.totalTasks === 0) return null

  const formatTime = (seconds: number) => {
    const hours = Math.floor(seconds / 3600)
    const minutes = Math.floor((seconds % 3600) / 60)
    if (hours > 0) return `${hours}h ${minutes}m`
    return `${minutes}m`
  }

  const maxAppCount = stats.mostUsedApps.length > 0 ? stats.mostUsedApps[0][1] : 1

  return (
    <div className="border-t border-white/[0.06] bg-black/30 backdrop-blur-sm">
      <motion.button
        type="button"
        whileTap={{ scale: 0.99 }}
        className="flex w-full items-center justify-between px-4 py-3 text-sm transition-colors hover:bg-white/[0.03]"
        onClick={() => setExpanded(!expanded)}
      >
        <div className="flex items-center gap-2">
          <ChartColumnIcon className="h-4 w-4 text-brand-400" />
          <span className="font-semibold text-white/85">{stats.totalTasks} tasks</span>
          <span className="text-white/25">·</span>
          <span className="text-white/50">{formatTime(stats.totalTimeSecs)}</span>
        </div>
        <motion.div animate={{ rotate: expanded ? 180 : 0 }} transition={{ duration: 0.2 }}>
          <ChevronDownIcon className="h-4 w-4 text-white/40" />
        </motion.div>
      </motion.button>

      <AnimatePresence initial={false}>
        {expanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
            className="overflow-hidden"
          >
            <div className="space-y-4 px-4 pb-4">
              {/* This Week */}
              <div className="grid grid-cols-2 gap-2">
                <StatTile icon={<BookOpenTextIcon className="h-3.5 w-3.5" />} label="This week" value={`${stats.tasksThisWeek}`} sub="tasks" />
                <StatTile icon={<ActivityIcon className="h-3.5 w-3.5" />} label="Time" value={formatTime(stats.timeThisWeekSecs)} sub="this week" />
              </div>

              {/* Most Used Apps */}
              {stats.mostUsedApps.length > 0 && (
                <div>
                  <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.1em] text-white/35">Top Apps</p>
                  <div className="space-y-1.5">
                    {stats.mostUsedApps.slice(0, 5).map(([app, count], i) => (
                      <div key={app} className="flex items-center gap-2">
                        <span className="w-20 truncate text-xs text-white/55">{app}</span>
                        <div className="flex-1">
                          <motion.div
                            initial={{ width: 0 }}
                            animate={{ width: `${Math.max(4, (count / maxAppCount) * 100)}%` }}
                            transition={{ duration: 0.5, delay: 0.05 * i, ease: [0.22, 1, 0.36, 1] }}
                            className="h-2 rounded-full bg-gradient-to-r from-brand-600 to-brand-400"
                          />
                        </div>
                        <span className="w-8 text-right font-mono text-xs text-white/45">{count}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* By Source */}
              {Object.keys(stats.tasksBySource).length > 0 && (
                <div>
                  <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.1em] text-white/35">By Source</p>
                  <div className="flex flex-wrap gap-2">
                    {Object.entries(stats.tasksBySource).map(([source, count]) => (
                      <motion.div
                        key={source}
                        initial={{ opacity: 0, scale: 0.9 }}
                        animate={{ opacity: 1, scale: 1 }}
                        transition={{ duration: 0.25 }}
                        className="flex items-center gap-1.5 rounded-full border border-white/[0.08] bg-white/[0.02] px-3 py-1"
                      >
                        <LayersIcon className="h-3 w-3 text-brand-400/70" />
                        <span className="text-xs text-white/65">{source}</span>
                        <span className="font-mono text-xs font-semibold text-white/80">{count}</span>
                      </motion.div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}

function StatTile({ icon, label, value, sub }: { icon: React.ReactNode; label: string; value: string; sub: string }) {
  return (
    <div className="rounded-xl border border-white/[0.06] bg-white/[0.015] p-3">
      <div className="flex items-center gap-2 text-xs text-white/40">
        {icon}
        {label}
      </div>
      <p className="mt-1 text-lg font-semibold text-white/85">{value}</p>
      <p className="text-xs text-white/35">{sub}</p>
    </div>
  )
}
