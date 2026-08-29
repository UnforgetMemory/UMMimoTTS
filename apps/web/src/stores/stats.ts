import { create } from 'zustand'
import type { ServerStats } from '@/api/endpoints'
import { fetchStats } from '@/api/endpoints'

interface StatsState {
  stats: ServerStats | null
  /** Timestamp of the last successful pull; drives the breaker countdown. */
  receivedAt: number
  loading: boolean
  error: string | null
  refresh: () => Promise<void>
}

export const useStatsStore = create<StatsState>((set) => ({
  stats: null,
  receivedAt: 0,
  loading: false,
  error: null,
  refresh: async () => {
    set({ loading: true })
    try {
      const stats = await fetchStats()
      set({ stats, receivedAt: Date.now(), loading: false, error: null })
    } catch (e) {
      set({ error: e instanceof Error ? e.message : String(e), loading: false })
    }
  },
}))
