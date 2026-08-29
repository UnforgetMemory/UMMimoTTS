import { create } from 'zustand'
import { TOKEN_KEY, getToken, setToken as persistToken } from '@/api/client'
import { clearScopedCache } from '@/api/scoped'

export interface AuthState {
  token: string | null
  setToken: (t: string) => void
  clearToken: () => void
}

export const useAuthStore = create<AuthState>((set) => ({
  token: getToken(),
  setToken: (t) => {
    persistToken(t)
    clearScopedCache() // invalidate short-lived scoped credentials on token change
    set({ token: t.trim() || null })
  },
  clearToken: () => {
    persistToken('')
    clearScopedCache()
    set({ token: null })
  },
}))

export { TOKEN_KEY }
