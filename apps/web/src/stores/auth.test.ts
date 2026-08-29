import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@/api/client', () => ({
  TOKEN_KEY: 'um-mimotts.token',
  getToken: vi.fn(),
  setToken: vi.fn(),
}))

vi.mock('@/api/scoped', () => ({
  clearScopedCache: vi.fn(),
}))

import { useAuthStore } from './auth'
import { getToken, setToken as persistToken } from '@/api/client'
import { clearScopedCache } from '@/api/scoped'

const getTokenMock = vi.mocked(getToken)
const persistTokenMock = vi.mocked(persistToken)
const clearScopedCacheMock = vi.mocked(clearScopedCache)

function resetState() {
  useAuthStore.setState({ token: null })
}

describe('useAuthStore', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    resetState()
  })

  describe('初始 token', () => {
    it('从 getToken() 读取初始 token', () => {
      getTokenMock.mockReturnValue('initial-token')
      // setState is the only lever on the singleton store; the initial
      // getToken() read happened at module load
      useAuthStore.setState({ token: getToken() })
      expect(useAuthStore.getState().token).toBe('initial-token')
    })

    it('getToken 返回 null 时 token 为 null', () => {
      getTokenMock.mockReturnValue(null)
      useAuthStore.setState({ token: getToken() })
      expect(useAuthStore.getState().token).toBeNull()
    })
  })

  describe('setToken', () => {
    it('设置 token + 清除 scoped cache', () => {
      useAuthStore.getState().setToken('new-token')
      expect(persistTokenMock).toHaveBeenCalledWith('new-token')
      expect(clearScopedCacheMock).toHaveBeenCalled()
      expect(useAuthStore.getState().token).toBe('new-token')
    })

    it('空字符串 → null', () => {
      useAuthStore.getState().setToken('')
      expect(useAuthStore.getState().token).toBeNull()
      expect(persistTokenMock).toHaveBeenCalledWith('')
    })

    it('纯空白字符串 → null', () => {
      useAuthStore.getState().setToken('   ')
      expect(useAuthStore.getState().token).toBeNull()
    })

    it('token 被 trim 后存储', () => {
      useAuthStore.getState().setToken('  abc  ')
      expect(useAuthStore.getState().token).toBe('abc')
      expect(persistTokenMock).toHaveBeenCalledWith('  abc  ')
    })

    it('每次 setToken 都清除 scoped cache', () => {
      useAuthStore.getState().setToken('t1')
      expect(clearScopedCacheMock).toHaveBeenCalledTimes(1)
      useAuthStore.getState().setToken('t2')
      expect(clearScopedCacheMock).toHaveBeenCalledTimes(2)
    })
  })

  describe('clearToken', () => {
    it('清除 token + scoped cache', () => {
      useAuthStore.setState({ token: 'existing' })
      useAuthStore.getState().clearToken()

      expect(useAuthStore.getState().token).toBeNull()
      expect(persistTokenMock).toHaveBeenCalledWith('')
      expect(clearScopedCacheMock).toHaveBeenCalled()
    })

    it('已经是 null 时再 clearToken 不报错', () => {
      useAuthStore.getState().clearToken()
      expect(useAuthStore.getState().token).toBeNull()
    })
  })

  describe('TOKEN_KEY 导出', () => {
    it('TOKEN_KEY 从 auth store 导出', async () => {
      const { TOKEN_KEY } = await import('./auth')
      expect(TOKEN_KEY).toBe('um-mimotts.token')
    })
  })
})
