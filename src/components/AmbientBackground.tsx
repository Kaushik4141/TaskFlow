/**
 * Static atmospheric layer behind the app.
 *
 * Keep the warmth of the original background without continuously animating
 * large blurred surfaces. A single composited gradient is substantially
 * cheaper while preserving the visual hierarchy behind the translucent panels.
 */
export default function AmbientBackground() {
  return (
    <div
      aria-hidden="true"
      className="pointer-events-none fixed inset-0 z-0 overflow-hidden bg-noir-950"
      style={{
        backgroundImage:
          'radial-gradient(circle at -4% -8%, rgb(var(--brand-500) / 0.18) 0%, rgb(var(--brand-400) / 0.06) 28%, transparent 52%), radial-gradient(circle at 104% 112%, rgb(var(--brand-600) / 0.16) 0%, rgb(var(--brand-300) / 0.05) 30%, transparent 58%), radial-gradient(ellipse at 50% 50%, rgb(var(--brand-100) / 0.025), transparent 60%)',
      }}
    />
  )
}
