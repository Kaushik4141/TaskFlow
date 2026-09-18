import { motion } from 'framer-motion'
import { CodeXmlIcon, XIcon } from '@animateicons/react/lucide'
import { modalVariants, useStaggerContainer, useStaggerItem } from '../lib/motion'

interface KeyboardShortcutsHelpProps {
  onClose: () => void
}

const shortcuts = [
  { keys: ['⌘/Ctrl', 'K'], description: 'Open search' },
  { keys: ['⌘/Ctrl', 'N'], description: 'New task' },
  { keys: ['⌘/Ctrl', 'Enter'], description: 'Start / stop active task' },
  { keys: ['⌘/Ctrl', ','], description: 'Open settings' },
  { keys: ['⌘/Ctrl', 'E'], description: 'Export documentation' },
  { keys: ['Esc'], description: 'Close any modal' },
  { keys: ['?'], description: 'Show this help' },
]

export default function KeyboardShortcutsHelp({ onClose }: KeyboardShortcutsHelpProps) {
  const containerV = useStaggerContainer(0.04)
  const itemV = useStaggerItem()

  return (
    <>
      <motion.div
        variants={modalVariants}
        initial="hidden"
        animate="visible"
        exit="exit"
        className="fixed inset-0 z-[90] bg-black/70 backdrop-blur-sm"
        onClick={onClose}
      />
      <motion.div
        variants={modalVariants}
        initial="hidden"
        animate="visible"
        exit="exit"
        className="fixed inset-0 z-[91] flex items-center justify-center p-4"
        onClick={onClose}
      >
        <div
          className="w-full max-w-md rounded-2xl border border-white/10 bg-noir-900/95 p-6 shadow-elevated backdrop-blur-xl"
          onClick={(e) => e.stopPropagation()}
        >
          <div className="mb-5 flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-brand-500/10 text-brand-300 ring-1 ring-brand-500/15">
                <CodeXmlIcon className="h-[18px] w-[18px]" />
              </div>
              <h2 className="text-lg font-semibold tracking-tight text-white">Keyboard Shortcuts</h2>
            </div>
            <button
              className="rounded-lg p-1.5 text-white/40 transition-colors hover:bg-white/5 hover:text-white"
              onClick={onClose}
              type="button"
            >
              <XIcon className="h-5 w-5" />
            </button>
          </div>

          <motion.div variants={containerV} initial="hidden" animate="show" className="space-y-2.5">
            {shortcuts.map(({ keys, description }) => (
              <motion.div
                key={description}
                variants={itemV}
                className="flex items-center justify-between"
              >
                <span className="text-sm text-white/65">{description}</span>
                <div className="flex items-center gap-1">
                  {keys.map((key, i) => (
                    <span key={i}>
                      {i > 0 && <span className="mx-1 text-white/25">+</span>}
                      <kbd className="inline-flex min-w-[28px] items-center justify-center rounded-md border border-white/10 bg-white/5 px-2 py-1 font-mono text-xs font-medium text-white/70">
                        {key}
                      </kbd>
                    </span>
                  ))}
                </div>
              </motion.div>
            ))}
          </motion.div>
        </div>
      </motion.div>
    </>
  )
}
