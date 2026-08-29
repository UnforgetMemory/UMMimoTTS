import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@/api/endpoints', () => ({
  fetchConfig: vi.fn(),
}))

import { useConfigStore } from './config'
import { fetchConfig, type Config } from '@/api/endpoints'

const fetchMock = vi.mocked(fetchConfig)

/** Reset store state (module-level inflight clears with reset too). */
function resetState() {
  useConfigStore.getState().reset()
}

describe('useConfigStore', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resetState()
  })

  it('load 首次调用 fetchConfig', async () => {
    const mockConfig = {
      voices: [],
      models: [],
      providers: [],
      default_voice: 'v1',
      default_model: 'm1',
      chunk: {},
      announcement: 'test',
    } as const

    fetchMock.mockResolvedValue(mockConfig as never)
    const result = await useConfigStore.getState().load()

    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(result).toBe(mockConfig)
    expect(useConfigStore.getState().config).toBe(mockConfig)
    expect(useConfigStore.getState().loading).toBe(false)
    expect(useConfigStore.getState().error).toBeNull()
  })

  it('已有 config 不重复请求', async () => {
    const config = {
      voices: [],
      models: [],
      providers: [],
      default_voice: 'v1',
      default_model: 'm1',
      chunk: {},
      announcement: null,
    } as const

    useConfigStore.setState({ config: config as never })
    fetchMock.mockResolvedValue(config as never)
    const result = await useConfigStore.getState().load()

    expect(fetchMock).not.toHaveBeenCalled()
    expect(result).toBe(config)
  })

  it('并发 load 去重（in-flight 只发一次请求）', async () => {
    let resolveFn!: (v: Config) => void
    fetchMock.mockImplementationOnce(() => {
      return new Promise((res) => {
        resolveFn = res
      })
    })

    const p1 = useConfigStore.getState().load()
    const p2 = useConfigStore.getState().load()

    expect(fetchMock).toHaveBeenCalledTimes(1)

    const mockConfig = {
      voices: [],
      models: [],
      providers: [],
      default_voice: 'v1',
      default_model: 'm1',
      chunk: {},
    } as const

    resolveFn(mockConfig as unknown as Config)
    const [r1, r2] = await Promise.all([p1, p2])
    expect(r1).toBe(mockConfig)
    expect(r2).toBe(mockConfig)
  })

  it('错误处理：设置 error，loading 复位', async () => {
    const err = new Error('网络错误')
    fetchMock.mockRejectedValue(err)

    const result = await useConfigStore.getState().load()
    expect(result).toBeNull()
    expect(useConfigStore.getState().error).toBe('网络错误')
    expect(useConfigStore.getState().loading).toBe(false)
    expect(useConfigStore.getState().config).toBeNull()
  })

  it('非 Error 异常用 String 转换', async () => {
    fetchMock.mockRejectedValue('string error')
    await useConfigStore.getState().load()
    expect(useConfigStore.getState().error).toBe('string error')
  })

  it('load 失败后 inflight 清除，再次 load 重新请求', async () => {
    fetchMock.mockRejectedValueOnce(new Error('boom'))
    await useConfigStore.getState().load()
    expect(useConfigStore.getState().error).toBe('boom')

    const config = {
      voices: [],
      models: [],
      providers: [],
      default_voice: 'v1',
      default_model: 'm1',
      chunk: {},
    } as const
    fetchMock.mockResolvedValue(config as never)
    const result = await useConfigStore.getState().load()

    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(result).toBe(config)
    expect(useConfigStore.getState().error).toBeNull()
  })

  it('reset 清除状态', async () => {
    useConfigStore.setState({ config: 'some-config' as never, loading: true, error: 'err' })
    useConfigStore.getState().reset()
    expect(useConfigStore.getState().config).toBeNull()
    expect(useConfigStore.getState().loading).toBe(false)
    expect(useConfigStore.getState().error).toBeNull()
  })

  it('reset 后 load 重新请求', async () => {
    const config = {
      voices: [],
      models: [],
      providers: [],
      default_voice: 'v1',
      default_model: 'm1',
      chunk: {},
    } as const

    fetchMock.mockResolvedValue(config as never)
    await useConfigStore.getState().load()
    expect(fetchMock).toHaveBeenCalledTimes(1)

    useConfigStore.getState().reset()
    fetchMock.mockResolvedValue(config as never)
    await useConfigStore.getState().load()
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })
})
