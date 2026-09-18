import { AnimatePresence, motion } from 'framer-motion'
import { CircleCheckIcon, InfoIcon, XIcon } from '@animateicons/react/lucide'
import { useToastStore, type Toast as ToastItem } from '../stores/toastStore'

export default function ToastContainer() {
  const { toasts, removeToast } = useToastStore()

  return (
    <div className="fixed bottom-4 right-4 z-[100] flex flex-col-reverse gap-2">
      <AnimatePresence initial={false}>
        {toasts.map((toast) => (
          <ToastNotification key={toast.id} toast={toast} onDismiss={() => removeToast(toast.id)} />
        ))}
      </AnimatePresence>
    </div>
  )
}

function ToastNotification({ toast, onDismiss }: { toast: ToastItem; onDismiss: () => void }) {
  const config = {
    success: {
      border: 'border-brand-500/40',
      bg: 'bg-brand-500/10',
      icon: <CircleCheckIcon className="h-5 w-5 text-brand-300" />,
      text: 'text-white',
    },
    error: {
      border: 'border-red-500/40',
      bg: 'bg-red-500/10',
      icon: <XIcon className="h-5 w-5 text-red-400" />,
      text: 'text-red-100',
    },
    info: {
      border: 'border-sky-500/40',
      bg: 'bg-sky-500/10',
      icon: <InfoIcon className="h-5 w-5 text-sky-400" />,
      text: 'text-sky-100',
    },
  }[toast.type]

  return (
    <motion.div
      layout
      initial={{ opacity: 0, y: 16, scale: 0.95 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={{ opacity: 0, x: 40, scale: 0.9 }}
      transition={{ type: 'spring', stiffness: 400, damping: 30 }}
      className={`flex min-w-[300px] max-w-md items-center gap-3 rounded-xl border ${config.border} ${config.bg} px-4 py-3 shadow-elevated backdrop-blur-md`}
    >
      {config.icon}
      <p className={`flex-1 text-sm ${config.text}`}>{toast.message}</p>
      <button
        className="rounded-lg p-1 text-white/40 transition-colors hover:bg-white/10 hover:text-white/80"
        onClick={onDismiss}
        type="button"
      >
        <XIcon className="h-4 w-4" />
      </button>
    </motion.div>
  )
}
