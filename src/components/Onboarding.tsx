import { useState } from 'react'
import { AnimatePresence, motion } from 'framer-motion'
import { invoke } from '@tauri-apps/api/core'
import { ActivityIcon, SquareArrowOutUpRightIcon, CircleCheckIcon, EyeIcon, BookOpenTextIcon, LinkIcon, LockIcon, PlayIcon, ShieldCheckIcon, SparklesIcon } from '@animateicons/react/lucide'
import AmbientBackground from './AmbientBackground'
import { selectionSpring } from '../lib/motion'

interface OnboardingProps {
  onComplete: () => void
}

const TOTAL_STEPS = 4

export default function Onboarding({ onComplete }: OnboardingProps) {
  const [step, setStep] = useState(0)
  const [deepCapture, setDeepCapture] = useState(false)
  const [direction, setDirection] = useState(1)

  const go = (next: number) => {
    setDirection(next > step ? 1 : -1)
    setStep(next)
  }

  const finish = async () => {
    try {
      await invoke('update_setting', { key: 'onboarding_completed', value: 'true' })
      if (deepCapture) {
        await invoke('update_privacy_settings', {
          excludedApps: ['1Password', 'Bitwarden'],
          captureClipboard: true,
          captureScreenText: true,
          captureWindowTitles: true,
        })
      }
    } catch {
      // Best effort
    }
    onComplete()
  }

  return (
    <div className="fixed inset-0 z-[110] flex items-center justify-center">
      <AmbientBackground />

      <div className="relative z-10 w-full max-w-lg px-6">
        {/* Progress dots */}
        <div className="mb-8 flex items-center justify-center gap-2">
          {Array.from({ length: TOTAL_STEPS }).map((_, i) => (
            <div key={i} className="relative h-1.5 overflow-hidden rounded-full bg-white/10">
              {i === step ? (
                <motion.div
                  layoutId="onboard-progress"
                  transition={selectionSpring}
                  className="absolute inset-0 rounded-full bg-gradient-to-r from-brand-500 to-brand-300"
                  style={{ width: 32 }}
                />
              ) : i < step ? (
                <div className="h-full w-2 rounded-full bg-brand-500/60" />
              ) : (
                <div className="h-full w-2 rounded-full bg-white/15" />
              )}
            </div>
          ))}
        </div>

        <AnimatePresence mode="wait" custom={direction}>
          {/* Step 0: Welcome */}
          {step === 0 && (
            <Slide key="welcome" direction={direction}>
              <div className="text-center">
                <div className="relative mx-auto mb-6 flex h-20 w-20 items-center justify-center rounded-2xl bg-gradient-to-br from-brand-500 to-brand-600 shadow-glow">
                  <ActivityIcon className="h-10 w-10 text-white" />
                  <div className="absolute -inset-2 -z-10 rounded-3xl bg-brand-500/30 blur-2xl" />
                </div>
                <h1 className="text-balance text-4xl font-bold tracking-tight text-white">
                  Welcome to <span className="bg-gradient-to-r from-brand-400 to-brand-200 bg-clip-text text-transparent">TaskFlow</span>
                </h1>
                <p className="mx-auto mt-4 max-w-sm text-base leading-7 text-white/55">
                  Documents how you work — automatically, 100% locally, open source.
                </p>
                <motion.button
                  type="button"
                  whileHover={{ y: -2 }}
                  whileTap={{ scale: 0.97 }}
                  className="mt-8 inline-flex items-center gap-2 rounded-xl border border-white/15 bg-white/[0.08] px-8 py-3 text-base font-semibold text-white shadow-elevated transition-all hover:border-white/25 hover:bg-white/[0.12]"
                  onClick={() => go(1)}
                >
                  Get Started
                  <SquareArrowOutUpRightIcon className="h-5 w-5" />
                </motion.button>
              </div>
            </Slide>
          )}

          {/* Step 1: How It Works */}
          {step === 1 && (
            <Slide key="how" direction={direction}>
              <h2 className="mb-8 text-center text-2xl font-bold tracking-tight text-white">How It Works</h2>
              <div className="space-y-3">
                {[
                  { icon: PlayIcon, title: 'Start a task', desc: 'Create or import a task from Jira, GitHub, or Linear.' },
                  { icon: EyeIcon, title: 'Work normally', desc: 'TaskFlow captures window switches, clipboard, and more in the background.' },
                  { icon: BookOpenTextIcon, title: 'Get documentation', desc: 'When you stop, AI generates structured documentation of what you did.' },
                ].map(({ icon: Icon, title, desc }, i) => (
                  <motion.div
                    key={title}
                    initial={{ opacity: 0, x: 20 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ delay: 0.1 + i * 0.08, duration: 0.35, ease: [0.22, 1, 0.36, 1] }}
                    className="flex items-start gap-4 rounded-xl border border-white/[0.06] bg-white/[0.02] p-4"
                  >
                    <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand-500/10 text-brand-300 ring-1 ring-brand-500/15">
                      <Icon className="h-5 w-5" />
                    </div>
                    <div>
                      <p className="font-semibold text-white/90">
                        <span className="mr-2 font-mono text-sm text-brand-400">{i + 1}.</span>
                        {title}
                      </p>
                      <p className="mt-1 text-sm leading-6 text-white/50">{desc}</p>
                    </div>
                  </motion.div>
                ))}
              </div>
              <NavButtons onBack={() => go(0)} onNext={() => go(2)} nextLabel="Next" />
            </Slide>
          )}

          {/* Step 2: Privacy */}
          {step === 2 && (
            <Slide key="privacy" direction={direction}>
              <div className="mb-6 flex items-center justify-center">
                <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-brand-500/10 ring-1 ring-brand-500/15">
                  <ShieldCheckIcon className="h-7 w-7 text-brand-300" />
                </div>
              </div>
              <h2 className="mb-3 text-center text-2xl font-bold tracking-tight text-white">Privacy First</h2>
              <p className="mx-auto mb-6 max-w-sm text-center text-sm leading-6 text-white/55">
                Your data never leaves your device unless you explicitly choose to use Cloud AI with your own key.
              </p>

              <div className="mb-4 space-y-2">
                {[
                  { icon: SparklesIcon, label: 'Basic', desc: 'Fast, local, no AI needed' },
                  { icon: LockIcon, label: 'Local AI', desc: 'Ollama, 100% on-device' },
                  { icon: ShieldCheckIcon, label: 'Cloud AI', desc: 'Your own API key' },
                ].map(({ icon: Icon, label, desc }) => (
                  <div key={label} className="flex items-center gap-3 rounded-xl border border-white/[0.06] bg-white/[0.02] p-3">
                    <Icon className="h-4 w-4 text-brand-400" />
                    <span className="text-sm font-semibold text-white/85">{label}</span>
                    <span className="text-xs text-white/40">— {desc}</span>
                  </div>
                ))}
              </div>

              <label className="flex cursor-pointer items-center justify-between rounded-xl border border-white/[0.06] bg-white/[0.02] p-4">
                <div>
                  <p className="text-sm font-semibold text-white/90">Enable deep capture</p>
                  <p className="mt-0.5 text-xs text-white/45">Captures screen text and clipboard for richer documentation</p>
                </div>
                <ToggleSwitch checked={deepCapture} onChange={setDeepCapture} />
              </label>

              <NavButtons onBack={() => go(1)} onNext={() => go(3)} nextLabel="Next" />
            </Slide>
          )}

          {/* Step 3: Integrations */}
          {step === 3 && (
            <Slide key="integrations" direction={direction}>
              <div className="mb-6 flex items-center justify-center">
                <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-brand-500/10 ring-1 ring-brand-500/15">
                  <LinkIcon className="h-7 w-7 text-brand-300" />
                </div>
              </div>
              <h2 className="mb-3 text-center text-2xl font-bold tracking-tight text-white">Connect Integrations</h2>
              <p className="mx-auto mb-6 max-w-sm text-center text-sm leading-6 text-white/55">
                Connect Jira, GitHub, or Linear to import tasks automatically. You can also skip this and add tasks manually.
              </p>

              <div className="mb-6 space-y-3">
                {[
                  { name: 'Jira', color: 'text-sky-300 border-sky-500/30' },
                  { name: 'GitHub', color: 'text-white/80 border-white/20' },
                  { name: 'Linear', color: 'text-violet-300 border-violet-500/30' },
                ].map(({ name, color }) => (
                  <div key={name} className={`flex items-center justify-between rounded-xl border ${color} bg-white/[0.02] p-4`}>
                    <span className="font-semibold text-white/90">{name}</span>
                    <span className="text-xs text-white/40">Configure in Settings</span>
                  </div>
                ))}
              </div>

              <div className="flex justify-between">
                <button className="rounded-xl px-6 py-3 text-sm text-white/45 transition-colors hover:text-white" onClick={() => go(2)} type="button">
                  Back
                </button>
                <motion.button
                  type="button"
                  whileHover={{ y: -2 }}
                  whileTap={{ scale: 0.97 }}
                  className="inline-flex items-center gap-2 rounded-xl bg-gradient-to-r from-brand-500 to-brand-600 px-8 py-3 font-semibold text-white shadow-glow"
                  onClick={() => void finish()}
                >
                  <CircleCheckIcon className="h-5 w-5" />
                  Finish
                </motion.button>
              </div>
            </Slide>
          )}
        </AnimatePresence>
      </div>
    </div>
  )
}

function Slide({ children, direction }: { children: React.ReactNode; direction: number }) {
  return (
    <motion.div
      initial={{ opacity: 0, x: direction * 30 }}
      animate={{ opacity: 1, x: 0 }}
      exit={{ opacity: 0, x: direction * -30 }}
      transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
    >
      {children}
    </motion.div>
  )
}

function NavButtons({ onBack, onNext, nextLabel }: { onBack: () => void; onNext: () => void; nextLabel: string }) {
  return (
    <div className="mt-8 flex justify-between">
      <button className="rounded-xl px-6 py-3 text-sm text-white/45 transition-colors hover:text-white" onClick={onBack} type="button">
        Back
      </button>
      <motion.button
        type="button"
        whileHover={{ y: -2 }}
        whileTap={{ scale: 0.97 }}
        className="inline-flex items-center gap-2 rounded-xl bg-gradient-to-r from-brand-500 to-brand-600 px-8 py-3 font-semibold text-white shadow-glow"
        onClick={onNext}
      >
        {nextLabel}
        <SquareArrowOutUpRightIcon className="h-5 w-5" />
      </motion.button>
    </div>
  )
}

function ToggleSwitch({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <motion.div
      className={`relative h-6 w-11 cursor-pointer rounded-full transition-colors ${checked ? 'bg-brand-500' : 'bg-white/15'}`}
      onClick={() => onChange(!checked)}
    >
      <motion.div
        layout
        transition={{ type: 'spring', stiffness: 500, damping: 32 }}
        className={`absolute top-0.5 h-5 w-5 rounded-full bg-white shadow-md ${checked ? 'right-0.5' : 'left-0.5'}`}
      />
    </motion.div>
  )
}
