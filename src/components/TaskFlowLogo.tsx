import type { HTMLAttributes } from 'react'

export interface TaskFlowLogoProps extends HTMLAttributes<HTMLDivElement> {
  size?: 'xs' | 'sm' | 'md' | 'lg' | 'xl'
  showWordmark?: boolean
  fixedRed?: boolean
  iconClassName?: string
  wordmarkClassName?: string
}

export function PulseWaveIcon({ className = 'h-full w-full' }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
    >
      <polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
    </svg>
  )
}

/**
 * TaskFlow Logo:
 * Rounded-square brand icon containing a crisp white pulse-wave/activity glyph,
 * paired with the "TaskFlow" wordmark in warm off-white (--ink) using Studio Feixen Sans.
 */
export default function TaskFlowLogo({
  size = 'md',
  showWordmark = true,
  fixedRed = false,
  className = '',
  iconClassName = '',
  wordmarkClassName = '',
  ...props
}: TaskFlowLogoProps) {
  // Dimension & radius tokens per existing radii system (rounded-xl is 12px)
  const dimensions = {
    xs: {
      container: 'gap-1.5',
      icon: 'h-5 w-5 rounded-[6px]',
      svg: 'h-3 w-3 text-white',
      text: 'text-xs font-semibold',
    },
    sm: {
      container: 'gap-2',
      icon: 'h-6 w-6 rounded-[8px]',
      svg: 'h-3.5 w-3.5 text-white',
      text: 'text-sm font-semibold',
    },
    md: {
      container: 'gap-2.5',
      icon: 'h-8 w-8 rounded-xl shadow-glow-sm', // 12px radius per tailwind.config.js
      svg: 'h-4.5 w-4.5 text-white',
      text: 'text-[15px] font-bold tracking-tight',
    },
    lg: {
      container: 'gap-3',
      icon: 'h-12 w-12 rounded-xl shadow-glow',
      svg: 'h-6 w-6 text-white',
      text: 'text-xl font-bold tracking-tight',
    },
    xl: {
      container: 'gap-4',
      icon: 'h-20 w-20 rounded-2xl shadow-glow',
      svg: 'h-10 w-10 text-white',
      text: 'text-3xl font-bold tracking-tight',
    },
  }[size]

  const bgStyle = fixedRed
    ? 'bg-gradient-to-br from-[#EF4444] to-[#DC2626]'
    : 'bg-gradient-to-br from-brand-500 to-brand-600'

  return (
    <div className={`inline-flex items-center ${dimensions.container} ${className}`} {...props}>
      {/* Rounded-square icon container with white pulse-wave glyph */}
      <div
        className={`relative flex shrink-0 items-center justify-center ${dimensions.icon} ${bgStyle} ${iconClassName}`}
      >
        <PulseWaveIcon className={dimensions.svg} />
      </div>

      {/* TaskFlow wordmark in warm off-white (--ink) with Studio Feixen Sans display font */}
      {showWordmark && (
        <span
          className={`font-display text-ink select-none ${dimensions.text} ${wordmarkClassName}`}
        >
          TaskFlow
        </span>
      )}
    </div>
  )
}
