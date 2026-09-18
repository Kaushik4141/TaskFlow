import { useReducedMotion } from 'framer-motion'

/**
 * Atmospheric gradient layer behind all content.
 *
 * The defining visual of the new TaskFlow: soft, heavily-blurred
 * red→pink glows bleeding into warm near-black. Sits behind the app
 * at z-0; the app surfaces (z-10+) are translucent so the glow shows
 * through subtly. The two blobs drift slowly to give the surface life
 * without ever being animated on reduced-motion.
 */
export default function AmbientBackground() {
  const reduce = useReducedMotion()
  const anim = reduce ? undefined : 'animate-gradient-drift'
  const animDelayed = reduce ? undefined : 'animate-gradient-drift [animation-delay:-9s]'

  return (
    <div className="pointer-events-none fixed inset-0 z-0 overflow-hidden bg-noir-950">
      {/* Primary theme glow — top-left */}
      <div
        className={`absolute -left-[12%] -top-[18%] h-[55vh] w-[55vh] rounded-full opacity-50 blur-[120px] ${anim}`}
        style={{
          background:
            'radial-gradient(circle, rgb(var(--brand-500) / 0.55) 0%, rgb(var(--brand-400) / 0.22) 40%, transparent 70%)',
        }}
      />
      {/* Secondary deeper theme glow — bottom-right */}
      <div
        className={`absolute -right-[14%] bottom-[-20%] h-[60vh] w-[60vh] rounded-full opacity-40 blur-[130px] ${animDelayed}`}
        style={{
          background:
            'radial-gradient(circle, rgb(var(--brand-600) / 0.5) 0%, rgb(var(--brand-300) / 0.18) 45%, transparent 70%)',
        }}
      />
      {/* Tertiary faint accent — center, for depth */}
      <div
        className="absolute left-1/2 top-1/2 h-[40vh] w-[80vh] -translate-x-1/2 -translate-y-1/2 rounded-full opacity-[0.12] blur-[140px]"
        style={{
          background:
            'radial-gradient(ellipse, rgb(var(--brand-100) / 0.6) 0%, transparent 70%)',
        }}
      />
      {/* Fine grain overlay for texture (no turbulence filter — just noise via gradient) */}
      <div
        className="absolute inset-0 opacity-[0.025]"
        style={{
          backgroundImage:
            'url("data:image/svg+xml,%3Csvg xmlns=%27http://www.w3.org/2000/svg%27 width=%27120%27 height=%27120%27%3E%3Cfilter id=%27n%27%3E%3CfeTurbulence type=%27fractalNoise%27 baseFrequency=%270.9%27 numOctaves=%272%27/%3E%3C/filter%3E%3Crect width=%27100%25%27 height=%27100%25%27 filter=%27url(%23n)%27/%3E%3C/svg%3E")',
        }}
      />
    </div>
  )
}
