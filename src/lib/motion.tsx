import { motion, useReducedMotion, type Variants } from 'framer-motion'
import type { ReactNode } from 'react'

/**
 * Shared framer-motion primitives for TaskFlow.
 *
 * Motion philosophy: every animation conveys state — entrance, selection,
 * feedback, transition. Nothing decorative. Reduced motion collapses all
 * variants to instant crossfades.
 */

const EASE_OUT = [0.22, 1, 0.36, 1] as const
const EASE_OUT_QUART = [0.25, 1, 0.5, 1] as const

function useSafeVariants(variants: Variants): Variants {
  const reduce = useReducedMotion()
  if (!reduce) return variants
  // Collapse to instant — keep opacity for crossfade only.
  const collapse = (v: Record<string, unknown>): Record<string, unknown> => {
    const out: Record<string, unknown> = {}
    for (const [k, val] of Object.entries(v)) {
      if (k === 'opacity' || k === 'transition') {
        out[k] = val
      }
    }
    return out
  }
  const result: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(variants)) {
    result[k] = typeof v === 'object' && v ? collapse(v as Record<string, unknown>) : v
  }
  return result as Variants
}

/** Wrap staggered list children — items use `staggerItem` variants. */
export function useStaggerContainer(staggerChildren = 0.04, delayChildren = 0) {
  return useSafeVariants({
    hidden: { opacity: 1 },
    show: {
      opacity: 1,
      transition: { staggerChildren, delayChildren },
    },
  })
}

export function useStaggerItem() {
  return useSafeVariants({
    hidden: { opacity: 0, y: 8 },
    show: {
      opacity: 1,
      y: 0,
      transition: { duration: 0.32, ease: EASE_OUT_QUART },
    },
  })
}

/** Spring used for shared-layout / selection markers. */
export const selectionSpring = {
  type: 'spring',
  stiffness: 380,
  damping: 32,
} as const

/** Generic fade-up entrance for sections / panels. */
export function useFadeUp(delay = 0) {
  return useSafeVariants({
    hidden: { opacity: 0, y: 10 },
    show: { opacity: 1, y: 0, transition: { duration: 0.4, ease: EASE_OUT, delay } },
  })
}

/** Subtle entrance used for the main view swap (Tasks ⇄ Settings, Docs ⇄ Feed). */
export function useViewSwap() {
  return useSafeVariants({
    initial: { opacity: 0, y: 6 },
    animate: { opacity: 1, y: 0, transition: { duration: 0.28, ease: EASE_OUT } },
    exit: { opacity: 0, y: -6, transition: { duration: 0.18, ease: EASE_OUT } },
  })
}

/* ---------------------------------------------------------------- */
/* Pre-built motion components — drop-in, reuse across the app.     */
/* ---------------------------------------------------------------- */

/** Hover-lift + tap-press for cards / rows / buttons. */
export function HoverCard({
  children,
  className = '',
  ...rest
}: {
  children: ReactNode
  className?: string
} & React.ComponentProps<typeof motion.div>) {
  const reduce = useReducedMotion()
  return (
    <motion.div
      whileHover={reduce ? undefined : { y: -2 }}
      whileTap={reduce ? undefined : { scale: 0.995 }}
      transition={{ type: 'spring', stiffness: 400, damping: 28 }}
      className={className}
      {...rest}
    >
      {children}
    </motion.div>
  )
}

/** Animated backdrop for modals / overlays. */
export function Backdrop({
  children,
  className = '',
  onClick,
}: {
  children?: ReactNode
  className?: string
  onClick?: () => void
}) {
  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.2 }}
      onClick={onClick}
      className={className}
    >
      {children}
    </motion.div>
  )
}

/** Modal entrance — scale + fade from center. */
export const modalVariants: Variants = {
  hidden: { opacity: 0, scale: 0.96, y: 8 },
  visible: { opacity: 1, scale: 1, y: 0, transition: { duration: 0.24, ease: EASE_OUT_QUART } },
  exit: { opacity: 0, scale: 0.97, y: 4, transition: { duration: 0.16, ease: EASE_OUT } },
}

/** Sheet/panel slide — for settings ↔ task swap & expand regions. */
export const slideVariants: Variants = {
  initial: { opacity: 0, x: -8 },
  animate: { opacity: 1, x: 0, transition: { duration: 0.26, ease: EASE_OUT } },
  exit: { opacity: 0, x: 8, transition: { duration: 0.16 } },
}

export { motion }
