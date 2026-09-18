import { getCurrentWindow } from '@tauri-apps/api/window'
import { MinusIcon, KeySquareIcon, XIcon } from '@animateicons/react/lucide'

interface WindowControlsProps {
  className?: string
}

export default function WindowControls({ className }: WindowControlsProps = {}) {
  const appWindow = getCurrentWindow()

  return (
    <div data-tauri-drag-region className={className ?? "absolute right-0 top-0 z-[60] flex h-8 items-center justify-end"}>
      <button
        type="button"
        className="inline-flex h-full w-11 items-center justify-center text-white/50 transition-colors hover:bg-white/10 hover:text-white"
        onClick={() => void appWindow.minimize()}
      >
        <MinusIcon className="h-[14px] w-[14px]" />
      </button>
      <button
        type="button"
        className="inline-flex h-full w-11 items-center justify-center text-white/50 transition-colors hover:bg-white/10 hover:text-white"
        onClick={() => void appWindow.toggleMaximize()}
      >
        <KeySquareIcon className="h-[12px] w-[12px]" />
      </button>
      <button
        type="button"
        className="inline-flex h-full w-11 items-center justify-center text-white/50 transition-colors hover:bg-red-500 hover:text-white"
        onClick={() => void appWindow.close()}
      >
        <XIcon className="h-4 w-4" />
      </button>
    </div>
  )
}
