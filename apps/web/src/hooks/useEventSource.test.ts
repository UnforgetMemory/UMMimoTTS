import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import type { DomainEvent } from '@/api/events'
import { useEventSource } from './useEventSource'

// —— Module mocks ——
vi.mock('@/api/sse', () => ({
  buildEventUrl: vi.fn(),
}))

vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn(),
}))

vi.mock('@/lib/backoff', () => ({
  nextBackoffMs: vi.fn(() => 1),
}))

vi.mock('@/lib/sse', () => ({
  parseSseMessage: vi.fn(),
}))

import { buildEventUrl } from '@/api/sse'
import { useAuthStore } from '@/stores/auth'
import type { AuthState } from '@/stores/auth'
import { nextBackoffMs } from '@/lib/backoff'
import { parseSseMessage } from '@/lib/sse'

const mockedBuildEventUrl = vi.mocked(buildEventUrl)
const mockedUseAuthStore = vi.mocked(useAuthStore)
const mockedNextBackoffMs = vi.mocked(nextBackoffMs)
const mockedParseSseMessage = vi.mocked(parseSseMessage)

/** Create a controllable EventSource mock. */
function createMockES() {
  return {
    onopen: null as (() => void) | null,
    onmessage: null as ((ev: { data: string }) => void) | null,
    onerror: null as (() => void) | null,
    close: vi.fn(),
  } as MockEventSource
}

interface MockEventSource {
  onopen: (() => void) | null
  onmessage: ((ev: { data: string }) => void) | null
  onerror: (() => void) | null
  close: () => void
}

function mockAuth(token = 'tk') {
  mockedUseAuthStore.mockImplementation(
    (selector: (s: AuthState) => unknown) =>
      selector({ token, setToken: vi.fn(), clearToken: vi.fn() }),
  )
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] })
  mockedBuildEventUrl.mockReset()
  mockedBuildEventUrl.mockResolvedValue('/api/v3/events?channel=test&token=tk')
  mockedNextBackoffMs.mockReturnValue(1)
  mockedParseSseMessage.mockReset()
  mockAuth()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('useEventSource', () => {
  it('channel 为 null 时状态为 closed', () => {
    const onEvent = vi.fn()
    const { result } = renderHook(() =>
      useEventSource({ channel: null, onEvent }),
    )
    expect(result.current.status).toBe('closed')
    expect(mockedBuildEventUrl).not.toHaveBeenCalled()
  })

  it('enabled=false 时状态为 closed', () => {
    const onEvent = vi.fn()
    const { result } = renderHook(() =>
      useEventSource({ channel: 'test', onEvent, enabled: false }),
    )
    expect(result.current.status).toBe('closed')
    expect(mockedBuildEventUrl).not.toHaveBeenCalled()
  })

  it('channel 有效时创建 EventSource 并设为 connecting → open', async () => {
    const onEvent = vi.fn()
    let capturedES: MockEventSource | null = null
    const factory = (url: string) => {
      capturedES = createMockES()
      return capturedES
    }

    const { result } = renderHook(() =>
      useEventSource({
        channel: 'test',
        onEvent,
        eventSourceFactory: factory as any,
      }),
    )

    // wait for async URL resolution
    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    expect(result.current.status).toBe('connecting')
    expect(mockedBuildEventUrl).toHaveBeenCalledWith('test')
    expect(capturedES).not.toBeNull()

    // fire open
    await act(async () => {
      capturedES!.onopen?.()
    })

    expect(result.current.status).toBe('open')
  })

  it('onmessage 触发时解析并调用 onEvent', async () => {
    const onEvent = vi.fn()
    const event: DomainEvent = {
      type: 'task_status_changed',
      task_id: 't1',
      session_id: null,
      status: 'done',
    }
    mockedParseSseMessage.mockReturnValue(event as any)

    let capturedES: MockEventSource | null = null
    const factory = (url: string) => {
      capturedES = createMockES()
      return capturedES
    }

    renderHook(() =>
      useEventSource({
        channel: 'test',
        onEvent,
        eventSourceFactory: factory as any,
      }),
    )

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    await act(async () => {
      capturedES!.onmessage?.({ data: '{"type":"task_status_changed"}' })
    })

    expect(mockedParseSseMessage).toHaveBeenCalledWith('{"type":"task_status_changed"}')
    expect(onEvent).toHaveBeenCalledWith(event)
  })

  it('onerror 触发时重连（backoff）', async () => {
    const onEvent = vi.fn()
    let capturedES: MockEventSource | null = null
    let callCount = 0
    const factory = (url: string) => {
      capturedES = createMockES()
      callCount++
      return capturedES
    }

    const { result } = renderHook(() =>
      useEventSource({
        channel: 'test',
        onEvent,
        eventSourceFactory: factory as any,
      }),
    )

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    expect(callCount).toBe(1)

    // fire error
    await act(async () => {
      capturedES!.onerror?.()
    })

    // wait for the backoff timer
    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    // should reconnect
    expect(callCount).toBe(2)
    expect(mockedNextBackoffMs).toHaveBeenCalled()
    expect(result.current.status).toBe('connecting')
  })

  it('reconnect() 函数触发重连', async () => {
    const onEvent = vi.fn()
    const esInstances: MockEventSource[] = []
    const factory = (url: string) => {
      const es = createMockES()
      esInstances.push(es)
      return es
    }

    const { result } = renderHook(() =>
      useEventSource({
        channel: 'test',
        onEvent,
        eventSourceFactory: factory as any,
      }),
    )

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    expect(mockedBuildEventUrl).toHaveBeenCalledTimes(1)
    expect(esInstances).toHaveLength(1)

    // invoke reconnect
    await act(async () => {
      result.current.reconnect()
    })

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    // a new EventSource should be created
    expect(mockedBuildEventUrl).toHaveBeenCalledTimes(2)
    expect(esInstances).toHaveLength(2)
    // the old ES should be closed
    expect(esInstances[0].close).toHaveBeenCalled()
  })

  it('cleanup 时关闭连接', async () => {
    const onEvent = vi.fn()
    let capturedES: MockEventSource | null = null
    const factory = (url: string) => {
      capturedES = createMockES()
      return capturedES
    }

    const { unmount } = renderHook(() =>
      useEventSource({
        channel: 'test',
        onEvent,
        eventSourceFactory: factory as any,
      }),
    )

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    expect(capturedES).not.toBeNull()
    expect(capturedES!.close).not.toHaveBeenCalled()

    unmount()
    expect(capturedES!.close).toHaveBeenCalled()
  })

  it('onStatusChange 回调在状态变化时被调用', async () => {
    const onEvent = vi.fn()
    const onStatusChange = vi.fn()
    let capturedES: MockEventSource | null = null
    const factory = (url: string) => {
      capturedES = createMockES()
      return capturedES
    }

    renderHook(() =>
      useEventSource({
        channel: 'test',
        onEvent,
        onStatusChange,
        eventSourceFactory: factory as any,
      }),
    )

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    expect(onStatusChange).toHaveBeenCalledWith('connecting')

    await act(async () => {
      capturedES!.onopen?.()
    })

    expect(onStatusChange).toHaveBeenCalledWith('open')
  })

  it('token 变化时重连', async () => {
    const onEvent = vi.fn()
    const esInstances: MockEventSource[] = []
    const factory = (url: string) => {
      const es = createMockES()
      esInstances.push(es)
      return es
    }

    // initial render (token='tk1')
    renderHook(
      ({ token }: { token: string }) =>
        useEventSource({
          channel: 'test',
          onEvent,
          eventSourceFactory: factory as any,
        }),
      {
        initialProps: { token: 'tk1' },
      },
    )

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    expect(mockedBuildEventUrl).toHaveBeenCalledTimes(1)
    expect(esInstances).toHaveLength(1)

    // update token (re-mock then rerender)
    mockAuth('tk2')

    // rerender the hook so the effect re-runs
    const { rerender } = renderHook(
      ({ token }: { token: string }) =>
        useEventSource({
          channel: 'test',
          onEvent,
          eventSourceFactory: factory as any,
        }),
      {
        initialProps: { token: 'tk2' },
      },
    )

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    expect(mockedBuildEventUrl).toHaveBeenCalledTimes(2)
    expect(esInstances).toHaveLength(2)
  })
})
