import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('./scoped', () => ({
  getScopedToken: vi.fn(),
}))

import { buildEventUrl } from './sse'
import { getScopedToken } from './scoped'

const scopedMock = vi.mocked(getScopedToken)

describe('buildEventUrl', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('正常构建 URL', async () => {
    scopedMock.mockResolvedValue('tok123')
    const url = await buildEventUrl('providers')
    expect(url).toBe('/api/v3/events?channel=providers&token=tok123')
    expect(scopedMock).toHaveBeenCalledWith('events:providers')
  })

  it('channel 含特殊字符时正确编码', async () => {
    scopedMock.mockResolvedValue('tok456')
    const url = await buildEventUrl('task:1 2+abc')
    // encodeURIComponent('task:1 2+abc') = 'task%3A1%202%2Babc'
    expect(url).toContain('channel=task%3A1%202%2Babc')
    expect(url).toContain('token=tok456')
  })

  it('getScopedToken 失败时抛出', async () => {
    const err = new Error('401 Unauthorized')
    scopedMock.mockRejectedValue(err)
    await expect(buildEventUrl('sessions:s1')).rejects.toThrow('401 Unauthorized')
    expect(scopedMock).toHaveBeenCalledWith('events:sessions:s1')
  })

  it('scope 前缀为 events:', async () => {
    scopedMock.mockResolvedValue('tok')
    await buildEventUrl('my-channel')
    expect(scopedMock).toHaveBeenCalledWith('events:my-channel')
  })
})
