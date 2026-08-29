import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { clearScopedCache, getScopedToken } from './scoped'

function okResponse(token: string, expires_in = 60, scope = 'audio:1'): Response {
  return new Response(JSON.stringify({ token, expires_in, scope }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  })
}

function errResponse(status: number, code: string): Response {
  return new Response(JSON.stringify({ error: 'x', code }), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const fetchMock = vi.fn()

beforeEach(() => {
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
  clearScopedCache()
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

describe('getScopedToken', () => {
  it('按 scope 缓存：TTL 内二次调用不再请求', async () => {
    fetchMock.mockResolvedValueOnce(okResponse('t1'))
    const a = await getScopedToken('audio:1')
    const b = await getScopedToken('audio:1')
    expect(a).toBe('t1')
    expect(b).toBe('t1')
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('并发同 scope 去重：只发一次请求', async () => {
    let resolveFn!: (r: Response) => void
    fetchMock.mockImplementationOnce(() => new Promise<Response>((res) => (resolveFn = res)))
    const p1 = getScopedToken('audio:2')
    const p2 = getScopedToken('audio:2')
    resolveFn(okResponse('t2'))
    await expect(Promise.all([p1, p2])).resolves.toEqual(['t2', 't2'])
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  it('401 时清空缓存并抛出，下次可重取', async () => {
    fetchMock.mockResolvedValueOnce(errResponse(401, 'UNAUTHORIZED'))
    await expect(getScopedToken('audio:3')).rejects.toThrow()
    expect(fetchMock).toHaveBeenCalledTimes(1)

    fetchMock.mockResolvedValueOnce(okResponse('t3'))
    await expect(getScopedToken('audio:3')).resolves.toBe('t3')
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })

  it('TTL 过期（expires_in - 30s 提前量）后重新请求', async () => {
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(new Date('2026-01-01T00:00:00Z'))

    fetchMock.mockResolvedValueOnce(okResponse('t5', 60, 'audio:5'))
    await getScopedToken('audio:5')
    await getScopedToken('audio:5')
    expect(fetchMock).toHaveBeenCalledTimes(1)

    vi.setSystemTime(new Date('2026-01-01T00:00:31Z')) // +31s, past the 30s leeway
    fetchMock.mockResolvedValueOnce(okResponse('t5b', 60, 'audio:5'))
    await getScopedToken('audio:5')
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })
})
