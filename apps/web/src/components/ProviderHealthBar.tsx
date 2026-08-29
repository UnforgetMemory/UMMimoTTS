import { useEffect, useState } from 'react'
import { anyOpenProvider, queueDepth, remainingOpenSecs, workerCount } from '@/lib/stats'
import { useStatsStore } from '@/stores/stats'
import { formatCountdown } from '@/lib/status'

/** Top-bar runtime indicator: /stats (5s poll) → queue depth/worker count
 * + provider breaker cooldown countdown. */
export function ProviderHealthBar() {
  const stats = useStatsStore((s) => s.stats)
  const receivedAt = useStatsStore((s) => s.receivedAt)
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    // Only tick while a cooldown is actually running: a bare 1s re-render
    // loop on an idle page is pure waste.
    if (stats == null || remainingOpenSecs(stats, receivedAt) == null) return
    const t = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(t)
  }, [stats, receivedAt])

  const open = anyOpenProvider(stats)
  const remaining = remainingOpenSecs(stats, receivedAt, now)

  return (
    <div className="flex items-center gap-2">
      <span className="num inline-flex items-center gap-1 rounded-full border border-border bg-surface-2 px-2.5 py-1 text-xs text-ink-secondary">
        队列 <span className="font-medium text-ink">{queueDepth(stats)}</span>
        <span className="text-ink-tertiary">·</span>
        并发 <span className="font-medium text-ink">{workerCount(stats)}</span>
      </span>

      {stats == null ? (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-border bg-surface-2 px-2.5 py-1 text-xs font-medium text-ink-tertiary">
          状态未知
        </span>
      ) : remaining != null ? (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-red-500/30 bg-red-500/10 px-2.5 py-1 text-xs font-medium text-red-600 dark:text-red-400">
          <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-red-500" />
          熔断冷却 <span className="num">{formatCountdown(remaining)}</span>
        </span>
      ) : open ? (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/30 bg-amber-500/10 px-2.5 py-1 text-xs font-medium text-amber-600 dark:text-amber-400">
          <span className="h-1.5 w-1.5 rounded-full bg-amber-500" />
          熔断中
        </span>
      ) : (
        <span className="inline-flex items-center gap-1.5 rounded-full border border-green-500/30 bg-green-500/10 px-2.5 py-1 text-xs font-medium text-green-600 dark:text-green-400">
          <span className="h-1.5 w-1.5 rounded-full bg-green-500" />
          运行正常
        </span>
      )}
    </div>
  )
}
