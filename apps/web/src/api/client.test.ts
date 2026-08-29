import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { ApiError, API_PREFIX, TOKEN_KEY, getToken, setToken, authHeaders, parseError, authedFetch, api } from './client'

describe('ApiError', () => {
  it('构造器设置 name/code/message/status', () => {
    const err = new ApiError('NOT_FOUND', '任务不存在', 404)
    expect(err.name).toBe('ApiError')
    expect(err.code).toBe('NOT_FOUND')
    expect(err.message).toBe('任务不存在')
    expect(err.status).toBe(404)
    expect(err).toBeInstanceOf(Error)
  })
})

describe('常量', () => {
  it('API_PREFIX 为 /api/v3', () => {
    expect(API_PREFIX).toBe('/api/v3')
  })
  it('TOKEN_KEY 为 um-mimotts.token', () => {
    expect(TOKEN_KEY).toBe('um-mimotts.token')
  })
})

describe('getToken', () => {
  beforeEach(() => {
    localStorage.clear()
  })

  it('有 token 时返回字符串', () => {
    localStorage.setItem(TOKEN_KEY, 'my-token')
    expect(getToken()).toBe('my-token')
  })

  it('无 token 时返回 null', () => {
    expect(getToken()).toBeNull()
  })

  it('localStorage 异常时返回 null', () => {
    const orig = localStorage.getItem
    vi.spyOn(localStorage, 'getItem').mockImplementation(() => {
      throw new Error('blocked')
    })
    expect(getToken()).toBeNull()
    vi.restoreAllMocks()
  })
})

describe('setToken', () => {
  beforeEach(() => {
    localStorage.clear()
  })

  it('设置非空 token 写入 localStorage', () => {
    setToken('abc123')
    expect(localStorage.getItem(TOKEN_KEY)).toBe('abc123')
  })

  it('空字符串 → 清除 localStorage', () => {
    localStorage.setItem(TOKEN_KEY, 'old')
    setToken('')
    expect(localStorage.getItem(TOKEN_KEY)).toBeNull()
  })

  it('纯空白字符串 → 清除 localStorage', () => {
    localStorage.setItem(TOKEN_KEY, 'old')
    setToken('   ')
    expect(localStorage.getItem(TOKEN_KEY)).toBeNull()
  })

  it('token 前后空白被 trim', () => {
    setToken('  abc  ')
    expect(localStorage.getItem(TOKEN_KEY)).toBe('abc')
  })

  it('localStorage 异常时静默忽略', () => {
    vi.spyOn(localStorage, 'setItem').mockImplementation(() => {
      throw new Error('quota exceeded')
    })
    expect(() => setToken('test')).not.toThrow()
    vi.restoreAllMocks()
  })
})

describe('authHeaders', () => {
  beforeEach(() => {
    localStorage.clear()
  })

  it('有 token 时添加 Authorization Bearer 头', () => {
    localStorage.setItem(TOKEN_KEY, 'secret')
    const h = authHeaders()
    expect(h.get('Authorization')).toBe('Bearer secret')
  })

  it('无 token 时不加 Authorization', () => {
    const h = authHeaders()
    expect(h.get('Authorization')).toBeNull()
  })

  it('保留 extra headers', () => {
    localStorage.setItem(TOKEN_KEY, 'secret')
    const h = authHeaders({ 'X-Custom': 'foo' })
    expect(h.get('Authorization')).toBe('Bearer secret')
    expect(h.get('X-Custom')).toBe('foo')
  })
})

describe('parseError', () => {
  it('JSON body 解析 code + error', async () => {
    const res = new Response(JSON.stringify({ code: 'UNAUTHORIZED', error: '未授权' }), {
      status: 401,
      headers: { 'Content-Type': 'application/json' },
    })
    const err = await parseError(res)
    expect(err).toBeInstanceOf(ApiError)
    expect(err.code).toBe('UNAUTHORIZED')
    expect(err.message).toBe('未授权')
    expect(err.status).toBe(401)
  })

  it('仅 code 无 error → 使用默认消息', async () => {
    const res = new Response(JSON.stringify({ code: 'VALIDATION' }), {
      status: 400,
      headers: { 'Content-Type': 'application/json' },
    })
    const err = await parseError(res)
    expect(err.code).toBe('VALIDATION')
    expect(err.message).toBe('请求失败（HTTP 400）')
  })

  it('非 JSON 响应 → 使用默认错误', async () => {
    const res = new Response('Not Found', { status: 404 })
    const err = await parseError(res)
    expect(err.code).toBe('INTERNAL')
    expect(err.message).toBe('请求失败（HTTP 404）')
    expect(err.status).toBe(404)
  })

  it('空 JSON body → 使用默认错误', async () => {
    const res = new Response('{}', { status: 500, headers: { 'Content-Type': 'application/json' } })
    const err = await parseError(res)
    expect(err.code).toBe('INTERNAL')
    expect(err.message).toBe('请求失败（HTTP 500）')
  })
})

describe('authedFetch', () => {
  let fetchMock: ReturnType<typeof vi.fn>

  beforeEach(() => {
    localStorage.clear()
    fetchMock = vi.fn().mockResolvedValue(new Response('{}', { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('拼接 API_PREFIX 并带 Bearer 头', async () => {
    localStorage.setItem(TOKEN_KEY, 'tok')
    await authedFetch('/tasks')
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/v3/tasks')
    const headers = init?.headers as Headers
    expect(headers.get('Authorization')).toBe('Bearer tok')
  })
})

describe('request via api', () => {
  let fetchMock: ReturnType<typeof vi.fn>

  beforeEach(() => {
    localStorage.clear()
    fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('网络错误 → ApiError NETWORK', async () => {
    fetchMock.mockRejectedValue(new TypeError('network down'))
    await expect(api.get('/x')).rejects.toMatchObject({ code: 'NETWORK', status: 0 })
  })

  it('401 → notifyUnauthorized 且抛 ApiError', async () => {
    const dispatchSpy = vi.spyOn(window, 'dispatchEvent').mockImplementation(() => true)
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify({ code: 'UNAUTHORIZED', error: '未授权' }), { status: 401 }),
    )
    await expect(api.get('/x')).rejects.toMatchObject({ code: 'UNAUTHORIZED', status: 401 })
    expect(dispatchSpy).toHaveBeenCalled()
    const ev = dispatchSpy.mock.calls[0]?.[0] as CustomEvent
    expect(ev.type).toBe('um-mimotts:unauthorized')
  })

  it('204 → undefined', async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 204 }))
    await expect(api.del('/x')).resolves.toBeUndefined()
  })

  it('空 body → undefined', async () => {
    fetchMock.mockResolvedValue(new Response('', { status: 200 }))
    await expect(api.get('/x')).resolves.toBeUndefined()
  })

  it('非法 JSON → ApiError BAD_RESPONSE', async () => {
    fetchMock.mockResolvedValue(new Response('not-json', { status: 200 }))
    await expect(api.get('/x')).rejects.toMatchObject({ code: 'BAD_RESPONSE' })
  })

  it('成功解析 JSON', async () => {
    fetchMock.mockResolvedValue(new Response('{"ok":true}', { status: 200 }))
    await expect(api.get('/x')).resolves.toEqual({ ok: true })
  })
})
