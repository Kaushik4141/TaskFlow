import { create } from 'zustand'

export type ToastType = 'success' | 'error' | 'info'

export interface Toast {
  id: string
  type: ToastType
  message: string
  persistent?: boolean
}

interface ToastStore {
  toasts: Toast[]
  addToast: (type: ToastType, message: string, persistent?: boolean) => string
  removeToast: (id: string) => void
}

let nextId = 0

export const useToastStore = create<ToastStore>((set, get) => ({
  toasts: [],

  addToast: (type, message, persistent = false) => {
    const id = `toast-${++nextId}`
    const toast: Toast = { id, type, message, persistent }

    set((state) => {
      const next = [...state.toasts, toast]
      while (next.length > 3) {
        const oldestIndex = next.findIndex((t) => !t.persistent)
        if (oldestIndex >= 0) {
          next.splice(oldestIndex, 1)
        } else {
          next.shift()
        }
      }
      return { toasts: next }
    })

    if (!persistent) {
      window.setTimeout(() => {
        get().removeToast(id)
      }, 4000)
    }

    return id
  },

  removeToast: (id) => {
    set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) }))
  },
}))
