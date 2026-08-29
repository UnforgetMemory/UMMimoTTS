import { create } from 'zustand'
import type { Config } from '@/api/endpoints'
import { fetchConfig } from '@/api/endpoints'

interface ConfigState {
  config: Config | null
  loading: boolean
  error: string | null
  load: () => Promise<Config | null>
  reset: () => void
}

// Module-level in-flight cache: Shell and Workbench may request config
// concurrently; a single fetch serves both.
let inflight: Promise<Config | null> | null = null

export const useConfigStore = create<ConfigState>((set, get) => ({
  config: null,
  loading: false,
  error: null,
  load: () => {
    if (get().config) return Promise.resolve(get().config)
    if (inflight) return inflight
    set({ loading: true, error: null })
    inflight = fetchConfig()
      .then((config) => {
        set({ config, loading: false })
        return config
      })
      .catch((e: unknown) => {
        const message = e instanceof Error ? e.message : String(e)
        set({ error: message, loading: false })
        return null
      })
      .finally(() => {
        inflight = null
      })
    return inflight
  },
  reset: () => {
    inflight = null
    set({ config: null, loading: false, error: null })
  },
}))
