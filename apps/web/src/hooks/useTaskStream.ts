import { useEffect, useRef } from 'react'
import type { DomainEvent } from '@/api/events'
import { buildEventUrl } from '@/api/sse'
import { useAuthStore } from '@/stores/auth'
import { nextBackoffMs } from '@/lib/backoff'
import { parseSseMessage } from '@/lib/sse'

export interface UseTaskStreamOptions {
  /** Task ids to subscribe to (usually the non-terminal ones only). */
  taskIds: string[]
  onEvent: (event: DomainEvent) => void
  eventSourceFactory?: (url: string) => EventSource
}

/**
 * Multi-task SSE subscription: one task:{id} channel per taskId with
 * automatic reconnect (exponential backoff).
 *
 * Connections reconcile incrementally per id: taskIds changes only
 * add/remove the affected channels, the rest stay alive — one finished task
 * on a multi-task board never triggers a reconnect storm across the others.
 * Backoff counts are per-id (attempts Map): onopen resets, onerror bumps.
 * A token change invalidates every scoped credential → all channels rebuild.
 */
export function useTaskStream(options: UseTaskStreamOptions): void {
  const { taskIds, onEvent } = options
  const token = useAuthStore((s) => s.token)

  const onEventRef = useRef(onEvent)
  onEventRef.current = onEvent
  const factoryRef = useRef(options.eventSourceFactory)
  factoryRef.current = options.eventSourceFactory

  // Registry survives taskIds changes (incremental reconcile, not rebuild).
  const sourcesRef = useRef(new Map<string, EventSource>())
  const timersRef = useRef(new Map<string, ReturnType<typeof setTimeout>>())
  const attemptsRef = useRef(new Map<string, number>())
  const taskIdsRef = useRef(taskIds)
  taskIdsRef.current = taskIds
  const prevTokenRef = useRef(token)
  const disposedRef = useRef(false)

  const taskIdsKey = taskIds.join(',')

  useEffect(() => {
    disposedRef.current = false
    const sources = sourcesRef.current
    const timers = timersRef.current
    const attempts = attemptsRef.current
    const makeFactory = () => factoryRef.current ?? ((u: string) => new EventSource(u))

    const attach = (id: string, es: EventSource) => {
      es.onopen = () => attempts.set(id, 0)
      es.onmessage = (ev: MessageEvent) => {
        const parsed = parseSseMessage(ev.data)
        if (parsed) onEventRef.current(parsed)
      }
      es.onerror = () => {
        if (disposedRef.current) return
        es.close()
        sources.delete(id)
        schedule(id)
      }
    }

    const connect = (id: string) => {
      if (disposedRef.current || sources.has(id) || !taskIdsRef.current.includes(id)) return
      buildEventUrl(`task:${id}`)
        .then((url) => {
          if (disposedRef.current || sources.has(id) || !taskIdsRef.current.includes(id)) return
          const es = makeFactory()(url)
          sources.set(id, es)
          attach(id, es)
        })
        .catch(() => {
          if (disposedRef.current || sources.has(id) || !taskIdsRef.current.includes(id)) return
          schedule(id)
        })
    }

    const schedule = (id: string) => {
      if (disposedRef.current) return
      const attempt = attempts.get(id) ?? 0
      const delay = nextBackoffMs(attempt)
      attempts.set(id, attempt + 1)
      timers.set(id, setTimeout(() => connect(id), delay))
    }

    // Token change invalidates every scoped credential → full rebuild once.
    const tokenChanged = prevTokenRef.current !== token
    prevTokenRef.current = token
    if (tokenChanged) {
      for (const es of sources.values()) es.close()
      sources.clear()
      for (const t of timers.values()) clearTimeout(t)
      timers.clear()
      attempts.clear()
    }

    // Incremental reconcile against the live registry.
    const wanted = new Set(taskIds)
    for (const id of taskIds) connect(id)
    for (const id of [...sources.keys()]) {
      if (!wanted.has(id)) {
        sources.get(id)?.close()
        sources.delete(id)
      }
    }
    for (const id of [...timers.keys()]) {
      if (!wanted.has(id)) {
        clearTimeout(timers.get(id))
        timers.delete(id)
      }
    }

    return () => {
      disposedRef.current = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [taskIdsKey, token])

  // Unmount: tear down every live connection and timer.
  useEffect(() => {
    return () => {
      disposedRef.current = true
      const sources = sourcesRef.current
      for (const es of sources.values()) es.close()
      sources.clear()
      const timers = timersRef.current
      for (const t of timers.values()) clearTimeout(t)
      timers.clear()
    }
  }, [])
}
