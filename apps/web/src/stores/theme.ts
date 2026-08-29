import { create } from 'zustand'

export type ThemeMode = 'dark' | 'light'
const STORAGE_KEY = 'um-mimotts.theme'

function readStoredTheme(): ThemeMode {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'light' ? 'light' : 'dark'
  } catch {
    return 'dark'
  }
}

function applyTheme(theme: ThemeMode): void {
  document.documentElement.classList.toggle('dark', theme === 'dark')
  try {
    localStorage.setItem(STORAGE_KEY, theme)
  } catch {
    /* ignore */
  }
}

interface ThemeState {
  theme: ThemeMode
  toggle: () => void
  setTheme: (t: ThemeMode) => void
}

export const useThemeStore = create<ThemeState>((set, get) => ({
  theme: readStoredTheme(),
  toggle: () => {
    const next: ThemeMode = get().theme === 'dark' ? 'light' : 'dark'
    applyTheme(next)
    set({ theme: next })
  },
  setTheme: (t) => {
    applyTheme(t)
    set({ theme: t })
  },
}))

/** Called once before React mounts: align html.dark with localStorage. */
export function initTheme(): void {
  applyTheme(readStoredTheme())
}
