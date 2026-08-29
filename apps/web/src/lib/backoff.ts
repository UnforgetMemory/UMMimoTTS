// SSE auto-reconnect exponential backoff (1s → 30s cap) + full jitter
// (ADR-012).

export const BACKOFF_BASE_MS = 1000
export const BACKOFF_MAX_MS = 30000

/**
 * Next reconnect delay: backoff = min(base * 2^attempt, max),
 * sleep = floor(random() * backoff) with a 1ms floor — random() ≈ 0 must
 * not produce an immediate tight reconnect loop.
 * `random` is injectable for tests.
 */
export function nextBackoffMs(attempt: number, random: () => number = Math.random): number {
  const n = Math.max(0, Math.floor(attempt))
  const backoff = Math.min(BACKOFF_BASE_MS * 2 ** n, BACKOFF_MAX_MS)
  return Math.max(1, Math.floor(random() * backoff))
}
