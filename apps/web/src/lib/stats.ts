// /stats pure helpers (engine runtime stats → display fields).
import type { ServerStats } from '@/api/endpoints'

export function queueDepth(stats: ServerStats | null): number {
  return stats?.queue_depth ?? 0
}

export function workerCount(stats: ServerStats | null): number {
  return stats?.workers ?? 0
}

export function anyOpenProvider(stats: ServerStats | null): boolean {
  return Boolean(stats?.providers?.some((p) => p.open))
}

/**
 * Breaker seconds remaining: retry_after_secs is the value at `receivedAt`,
 * decaying over time. Returns the longest cooldown among open providers,
 * or null when none is open.
 */
export function remainingOpenSecs(
  stats: ServerStats | null,
  receivedAt: number,
  now: number = Date.now(),
): number | null {
  if (!stats?.providers) return null
  const elapsed = (now - receivedAt) / 1000
  let max: number | null = null
  for (const p of stats.providers) {
    if (p.open && p.retry_after_secs != null) {
      const remain = Math.max(0, Math.ceil(p.retry_after_secs - elapsed))
      max = max == null ? remain : Math.max(max, remain)
    }
  }
  return max
}
