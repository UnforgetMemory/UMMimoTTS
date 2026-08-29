import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@/api/endpoints', () => ({
  fetchStats: vi.fn(),
}))

import { useStatsStore } from './stats'
import { fetchStats, type ServerStats } from '@/api/endpoints'

const fetchMock = vi.mocked(fetchStats)

function resetState() {
  useStatsStore.setState({ stats: null, receivedAt: 0, loading: false, error: null })
}

describe('useStatsStore', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resetState()
  })

  it('refresh 成功：设置 stats + receivedAt', async () => {
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(new Date('2026-01-01T00:00:00Z'))

    const mockStats = {
      queue_depth: 5,
      workers: 8,
      providers: [
        { provider_id: 'p1', open: false },
      ],
    } as const

    fetchMock.mockResolvedValue(mockStats as never)
    await useStatsStore.getState().refresh()

    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(useStatsStore.getState().stats).toBe(mockStats)
    expect(useStatsStore.getState().receivedAt).toBe(Date.now())
    expect(useStatsStore.getState().loading).toBe(false)
    expect(useStatsStore.getState().error).toBeNull()

    vi.useRealTimers()
  })

  it('refresh 失败：设置 error', async () => {
    const err = new Error('503 Service Unavailable')
    fetchMock.mockRejectedValue(err)

    await useStatsStore.getState().refresh()

    expect(useStatsStore.getState().error).toBe('503 Service Unavailable')
    expect(useStatsStore.getState().loading).toBe(false)
    expect(useStatsStore.getState().stats).toBeNull()
    expect(useStatsStore.getState().receivedAt).toBe(0)
  })

  it('非 Error 异常用 String 转换', async () => {
    fetchMock.mockRejectedValue('raw string')
    await useStatsStore.getState().refresh()
    expect(useStatsStore.getState().error).toBe('raw string')
  })

  it('loading 状态在请求期间为 true', async () => {
    let resolveFn!: (v: ServerStats) => void
    fetchMock.mockImplementationOnce(() => {
      return new Promise((res) => {
        resolveFn = res
      })
    })

    const promise = useStatsStore.getState().refresh()
    expect(useStatsStore.getState().loading).toBe(true)

    resolveFn({ queue_depth: 0, workers: 0 } as never)
    await promise
    expect(useStatsStore.getState().loading).toBe(false)
  })

  it('多次 refresh 不累积错误', async () => {
    fetchMock.mockResolvedValue({ queue_depth: 1, workers: 1 } as never)
    await useStatsStore.getState().refresh()
    expect(useStatsStore.getState().error).toBeNull()

    fetchMock.mockRejectedValue(new Error('fail'))
    await useStatsStore.getState().refresh()
    expect(useStatsStore.getState().error).toBe('fail')

    fetchMock.mockResolvedValue({ queue_depth: 2, workers: 2 } as never)
    await useStatsStore.getState().refresh()
    expect(useStatsStore.getState().error).toBeNull()
  })
})
