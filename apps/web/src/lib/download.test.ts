import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

vi.mock('@/api/client', () => ({
  authedFetch: vi.fn(),
  parseError: vi.fn(),
  notifyUnauthorized: vi.fn(),
  ApiError: class ApiError extends Error {
    code: string
    status: number
    constructor(code: string, message: string, status: number) {
      super(message)
      this.name = 'ApiError'
      this.code = code
      this.status = status
    }
  },
}))

import { downloadViaFetch } from './download'
import { authedFetch, parseError, notifyUnauthorized, ApiError } from '@/api/client'

const fetchMock = vi.mocked(authedFetch)
const parseErrorMock = vi.mocked(parseError)
const notifyUnauthorizedMock = vi.mocked(notifyUnauthorized)

describe('downloadViaFetch', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  it('正常下载：mock authedFetch 返回 ok blob，验证 a.download 和 click', async () => {
    const blob = new Blob(['hello'])
    vi.mocked(URL.createObjectURL).mockReturnValue('blob:mock-url')

    fetchMock.mockResolvedValue({
      status: 200,
      ok: true,
      blob: vi.fn().mockResolvedValue(blob),
    } as unknown as Response)

    // real DOM element to satisfy jsdom typing
    const clickSpy = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
    const appendChildSpy = vi.spyOn(document.body, 'appendChild').mockImplementation(() => document.createElement('div'))

    await downloadViaFetch('/sessions/s1/export', 'session-s1.zip')

    expect(fetchMock).toHaveBeenCalledWith('/sessions/s1/export')
    expect(URL.createObjectURL).toHaveBeenCalledWith(blob)
    expect(appendChildSpy).toHaveBeenCalled()
    expect(clickSpy).toHaveBeenCalled()
    const anchor = appendChildSpy.mock.calls[0]?.[0] as HTMLAnchorElement
    expect(anchor.download).toBe('session-s1.zip')

    // revokeObjectURL fires after the 1000ms delay
    expect(URL.revokeObjectURL).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1000)
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:mock-url')
  })

  it('401 时通知未授权', async () => {
    fetchMock.mockResolvedValue({
      status: 401,
      ok: false,
    } as unknown as Response)
    parseErrorMock.mockResolvedValue(new ApiError('UNAUTHORIZED', 'Unauthorized', 401))

    await expect(downloadViaFetch('/sessions/s1/export', 'file.zip')).rejects.toThrow('Unauthorized')
    expect(notifyUnauthorizedMock).toHaveBeenCalled()
    expect(fetchMock).toHaveBeenCalledWith('/sessions/s1/export')
  })

  it('非 ok 时抛出错误（parseError）', async () => {
    const apiError = new ApiError('NOT_FOUND', 'Not Found', 404)
    fetchMock.mockResolvedValue({
      status: 404,
      ok: false,
    } as unknown as Response)
    parseErrorMock.mockResolvedValue(apiError)

    await expect(downloadViaFetch('/tasks/t1/download', 'task.zip')).rejects.toBe(apiError)
    expect(parseErrorMock).toHaveBeenCalled()
  })

  it('URL.createObjectURL 被调用', async () => {
    const blob = new Blob(['data'])
    fetchMock.mockResolvedValue({
      status: 200,
      ok: true,
      blob: vi.fn().mockResolvedValue(blob),
    } as unknown as Response)

    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
    vi.spyOn(document.body, 'appendChild').mockImplementation(() => document.createElement('div'))

    await downloadViaFetch('/export', 'file.zip')
    expect(URL.createObjectURL).toHaveBeenCalledWith(blob)
  })
})
