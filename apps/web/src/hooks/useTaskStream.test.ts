import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import type { DomainEvent } from '@/api/events'
import { useTaskStream } from './useTaskStream'

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
const mockedParseSseMessage = vi.mocked(parseSseMessage)

/** Create a controllable EventSource mock. */
function createMockES() {
  const es: MockEventSource = {
    onopen: null,
    onmessage: null,
    onerror: null,
    close: vi.fn(),
  }
  return es
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
  mockedBuildEventUrl.mockResolvedValue('/api/v3/events?channel=task:t1&token=tk')
  mockedParseSseMessage.mockReset()
  mockAuth()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('useTaskStream', () => {
  it('taskIds 为空时不创建连接', () => {
    const onEvent = vi.fn()
    renderHook(() =>
      useTaskStream({ taskIds: [], onEvent }),
    )

    expect(mockedBuildEventUrl).not.toHaveBeenCalled()
  })

  it('为每个 taskId 创建 EventSource', async () => {
    const onEvent = vi.fn()
    const createdES: MockEventSource[] = []
    const factory = (url: string) => {
      const es = createMockES()
      createdES.push(es)
      return es
    }

    renderHook(() =>
      useTaskStream({
        taskIds: ['t1', 't2', 't3'],
        onEvent,
        eventSourceFactory: factory as any,
      }),
    )

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    expect(mockedBuildEventUrl).toHaveBeenCalledTimes(3)
    expect(mockedBuildEventUrl).toHaveBeenCalledWith('task:t1')
    expect(mockedBuildEventUrl).toHaveBeenCalledWith('task:t2')
    expect(mockedBuildEventUrl).toHaveBeenCalledWith('task:t3')
    expect(createdES).toHaveLength(3)
  })

  it('onmessage 触发时调用 onEvent', async () => {
    const onEvent = vi.fn()
    const event: DomainEvent = {
      type: 'task_completed',
      task_id: 't1',
      session_id: null,
      output_path: '/tmp/out.wav',
      duration_ms: 1000,
    }
    mockedParseSseMessage.mockReturnValue(event as any)

    const createdES: MockEventSource[] = []
    const factory = (url: string) => {
      const es = createMockES()
      createdES.push(es)
      return es
    }

    renderHook(() =>
      useTaskStream({
        taskIds: ['t1'],
        onEvent,
        eventSourceFactory: factory as any,
      }),
    )

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    await act(async () => {
      createdES[0].onmessage?.({ data: '{"type":"task_completed"}' })
    })

    expect(onEvent).toHaveBeenCalledWith(event)
  })

  it('cleanup 时关闭所有连接', async () => {
    const onEvent = vi.fn()
    const createdES: MockEventSource[] = []
    const factory = (url: string) => {
      const es = createMockES()
      createdES.push(es)
      return es
    }

    const { unmount } = renderHook(() =>
      useTaskStream({
        taskIds: ['t1', 't2'],
        onEvent,
        eventSourceFactory: factory as any,
      }),
    )

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    expect(createdES).toHaveLength(2)

    unmount()

    expect(createdES[0].close).toHaveBeenCalled()
    expect(createdES[1].close).toHaveBeenCalled()
  })

  it('taskIds 变化时重建连接', async () => {
    const onEvent = vi.fn()
    const createdES: MockEventSource[] = []
    const factory = (url: string) => {
      const es = createMockES()
      createdES.push(es)
      return es
    }

    const { rerender } = renderHook(
      ({ ids }: { ids: string[] }) =>
        useTaskStream({ taskIds: ids, onEvent, eventSourceFactory: factory as any }),
      { initialProps: { ids: ['t1'] } },
    )

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    expect(createdES).toHaveLength(1)
    expect(mockedBuildEventUrl).toHaveBeenCalledWith('task:t1')

    // clear mock call records
    mockedBuildEventUrl.mockClear()

    // update taskIds
    rerender({ ids: ['t2', 't3'] })

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    expect(createdES).toHaveLength(3) // old 1 + new 2
    expect(mockedBuildEventUrl).toHaveBeenCalledWith('task:t2')
    expect(mockedBuildEventUrl).toHaveBeenCalledWith('task:t3')
  })

  it('onopen 时重置退避计数', async () => {
    const onEvent = vi.fn()
    const createdES: MockEventSource[] = []
    const factory = (url: string) => {
      const es = createMockES()
      createdES.push(es)
      return es
    }

    renderHook(() =>
      useTaskStream({
        taskIds: ['t1'],
        onEvent,
        eventSourceFactory: factory as any,
      }),
    )

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    // fire error
    await act(async () => {
      createdES[0].onerror?.()
    })

    // wait for reconnect
    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    // fire open (resets attempts)
    await act(async () => {
      createdES[1].onopen?.()
    })

    // fire error again
    await act(async () => {
      createdES[1].onerror?.()
    })

    await act(async () => {
      vi.runOnlyPendingTimers()
    })

    // a fresh ES should be created
    expect(createdES).toHaveLength(3)
  })
})
