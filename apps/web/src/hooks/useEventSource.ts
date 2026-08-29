import { useEffect, useRef, useState } from 'react'
import type { DomainEvent } from '@/api/events'
import { buildEventUrl } from '@/api/sse'
import { useAuthStore } from '@/stores/auth'
import { nextBackoffMs } from '@/lib/backoff'
import { parseSseMessage } from '@/lib/sse'

export type ConnectionStatus = 'connecting' | 'open' | 'closed'

export interface UseEventSourceOptions {
  /** Channel to subscribe to, e.g. providers / task:${id} / session:${id}
   * (a scoped token is attached internally). */
  channel: string | null
  onEvent: (event: DomainEvent) => void
  onStatusChange?: (status: ConnectionStatus) => void
  enabled?: boolean
  /** Injectable EventSource factory (tests/SSR); defaults to the global. */
  eventSourceFactory?: (url: string) => EventSource
}

export interface UseEventSourceResult {
  status: ConnectionStatus
  reconnect: () => void
}

/**
 * SSE subscription hook: EventSource + auto-reconnect (exponential backoff
 * 1s→30s full jitter). Auth: the URL resolves a scoped token async
 * (events:{channel}); connection starts once ready and retries with backoff.
 */
export function useEventSource(options: UseEventSourceOptions): UseEventSourceResult {
  const { channel, onEvent, onStatusChange, enabled = true } = options
  const token = useAuthStore((s) => s.token)
  const [status, setStatus] = useState<ConnectionStatus>('closed')
  const [nonce, setNonce] = useState(0)

  const onEventRef = useRef(onEvent)
  onEventRef.current = onEvent
  const onStatusRef = useRef(onStatusChange)
  onStatusRef.current = onStatusChange
  const factoryRef = useRef(options.eventSourceFactory)
  factoryRef.current = options.eventSourceFactory

  const reconnect = () => setNonce((n) => n + 1)

  useEffect(() => {
    if (!channel || !enabled) {
      setStatus('closed')
      onStatusRef.current?.('closed')
      return
    }

    let disposed = false
    let source: EventSource | null = null
    let attempt = 0
    let timer: ReturnType<typeof setTimeout> | null = null

    const defaultFactory = (u: string) => new EventSource(u)
    const factory = factoryRef.current ?? defaultFactory

    const close = () => {
      if (timer) {
        clearTimeout(timer)
        timer = null
      }
      source?.close()
      source = null
    }

    const openWithUrl = (url: string) => {
      if (disposed) return
      source = factory(url)
      source.onopen = () => {
        if (disposed) return
        attempt = 0
        setStatus('open')
        onStatusRef.current?.('open')
      }
      source.onmessage = (ev: MessageEvent) => {
        const parsed = parseSseMessage(ev.data)
        if (parsed) onEventRef.current(parsed)
      }
      source.onerror = () => {
        if (disposed) return
        close()
        const delay = nextBackoffMs(attempt)
        attempt += 1
        timer = setTimeout(open, delay)
      }
    }

    const open = () => {
      if (disposed) return
      setStatus('connecting')
      onStatusRef.current?.('connecting')
      buildEventUrl(channel)
        .then((url) => {
          if (!disposed) openWithUrl(url)
        })
        .catch(() => {
          // scoped token fetch failed (401/network): retry with backoff
          // instead of spinning.
          if (disposed) return
          const delay = nextBackoffMs(attempt)
          attempt += 1
          timer = setTimeout(open, delay)
        })
    }

    open()

    return () => {
      disposed = true
      close()
    }
    // channel/enabled/nonce/token changes reopen the connection; callbacks
    // and the factory live in refs so no reconnect happens per render.
  }, [channel, enabled, nonce, token])

  return { status, reconnect }
}
