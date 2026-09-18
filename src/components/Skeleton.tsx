export function Skeleton({ className = '', width, height }: { className?: string; width?: string; height?: string }) {
  return (
    <div
      className={`animate-shimmer rounded-xl bg-gradient-to-r from-white/[0.02] via-white/[0.06] to-white/[0.02] bg-[length:200%_100%] ${className}`}
      style={{ width, height }}
    />
  )
}

export function SkeletonText({ lines = 3 }: { lines?: number }) {
  return (
    <div className="space-y-2">
      {Array.from({ length: lines }).map((_, i) => (
        <Skeleton
          key={i}
          className="h-4"
          width={i === lines - 1 ? '60%' : '100%'}
        />
      ))}
    </div>
  )
}

export function SkeletonCard() {
  return (
    <div className="rounded-xl border border-white/[0.04] bg-white/[0.01] p-4 space-y-3">
      <div className="flex items-center gap-3">
        <Skeleton className="h-3 w-3 rounded-full" />
        <Skeleton className="h-4 flex-1" />
        <Skeleton className="h-4 w-12" />
      </div>
      <Skeleton className="h-3 w-3/4" />
    </div>
  )
}

export function SkeletonDocument() {
  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-between">
        <div className="space-y-2">
          <Skeleton className="h-3 w-24" />
          <Skeleton className="h-6 w-48" />
        </div>
        <div className="flex gap-2">
          <Skeleton className="h-9 w-32 rounded-xl" />
          <Skeleton className="h-9 w-28 rounded-xl" />
        </div>
      </div>
      <Skeleton className="h-20 w-full rounded-xl" />
      <SkeletonText lines={5} />
      <SkeletonText lines={4} />
    </div>
  )
}
