import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { useAudioUrl } from './useAudioUrl'

// —— Module mocks ——
vi.mock('@/api/endpoints', () => ({
  taskAudioUrl: vi.fn(),
}))

vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn(),
}))

import { taskAudioUrl } from '@/api/endpoints'
import { useAuthStore } from '@/stores/auth'
import type { AuthState } from '@/stores/auth'

const mockedTaskAudioUrl = vi.mocked(taskAudioUrl)
const mockedUseAuthStore = vi.mocked(useAuthStore)

function mockAuth(token = 'tk') {
  mockedUseAuthStore.mockImplementation(
    (selector: (s: AuthState) => unknown) =>
      selector({ token, setToken: vi.fn(), clearToken: vi.fn() }),
  )
}

beforeEach(() => {
  mockedTaskAudioUrl.mockReset()
  mockAuth()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('useAudioUrl', () => {
  it('taskId 为 undefined 时 url 为 null', () => {
    const { result } = renderHook(() => useAudioUrl(undefined, true))
    expect(result.current.url).toBeNull()
    expect(mockedTaskAudioUrl).not.toHaveBeenCalled()
  })

  it('hasAudio=false 时 url 为 null', () => {
    const { result } = renderHook(() => useAudioUrl('t1', false))
    expect(result.current.url).toBeNull()
    expect(mockedTaskAudioUrl).not.toHaveBeenCalled()
  })

  it('taskId 有效时调用 taskAudioUrl 并设置 url', async () => {
    mockedTaskAudioUrl.mockResolvedValueOnce('/api/v3/tasks/t1/audio?token=tk')

    const { result } = renderHook(() => useAudioUrl('t1', true))

    expect(result.current.url).toBeNull()

    await act(async () => {
      await Promise.resolve()
    })

    expect(mockedTaskAudioUrl).toHaveBeenCalledWith('t1')
    expect(result.current.url).toBe('/api/v3/tasks/t1/audio?token=tk')
  })

  it('taskAudioUrl 失败时保持 null', async () => {
    mockedTaskAudioUrl.mockRejectedValueOnce(new Error('network error'))

    const { result } = renderHook(() => useAudioUrl('t1', true))

    await act(async () => {
      await Promise.resolve()
    })

    expect(result.current.url).toBeNull()
  })

  it('taskId 变化时重新获取', async () => {
    mockedTaskAudioUrl.mockResolvedValueOnce('/url1')
    mockedTaskAudioUrl.mockResolvedValueOnce('/url2')

    const { result, rerender } = renderHook(
      ({ id }: { id: string }) => useAudioUrl(id, true),
      { initialProps: { id: 't1' } },
    )

    await act(async () => {
      await Promise.resolve()
    })

    expect(result.current.url).toBe('/url1')
    expect(mockedTaskAudioUrl).toHaveBeenCalledWith('t1')

    // update taskId
    rerender({ id: 't2' })

    await act(async () => {
      await Promise.resolve()
    })

    expect(mockedTaskAudioUrl).toHaveBeenCalledWith('t2')
    expect(result.current.url).toBe('/url2')
  })

  it('token 变化时重新获取', async () => {
    mockedTaskAudioUrl.mockResolvedValueOnce('/url1')
    mockedTaskAudioUrl.mockResolvedValueOnce('/url2')

    // same renderHook; token change arrives via rerender
    mockedUseAuthStore.mockImplementation(
      (selector: (s: AuthState) => unknown) =>
        selector({ token: 'tk1', setToken: vi.fn(), clearToken: vi.fn() }),
    )

    const { result, rerender } = renderHook(
      () => useAudioUrl('t1', true),
    )

    await act(async () => {
      await Promise.resolve()
    })

    expect(result.current.url).toBe('/url1')
    expect(mockedTaskAudioUrl).toHaveBeenCalledWith('t1')

    // simulate token change: update mock then rerender
    mockedUseAuthStore.mockImplementation(
      (selector: (s: AuthState) => unknown) =>
        selector({ token: 'tk2', setToken: vi.fn(), clearToken: vi.fn() }),
    )

    rerender()

    await act(async () => {
      await Promise.resolve()
    })

    // taskAudioUrl should be called again after the token change
    expect(mockedTaskAudioUrl.mock.calls.length).toBeGreaterThanOrEqual(2)
  })
})
