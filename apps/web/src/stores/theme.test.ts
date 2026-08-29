import { describe, it, expect, beforeEach, vi } from 'vitest'
import { useThemeStore, initTheme } from './theme'
import type { ThemeMode } from './theme'

const STORAGE_KEY = 'um-mimotts.theme'

/** Reset the store to a given theme (bypasses localStorage writes). */
function resetStore(theme: ThemeMode) {
  useThemeStore.setState({ theme })
}

describe('useThemeStore', () => {
  beforeEach(() => {
    localStorage.clear()
    // clear the theme class on html
    document.documentElement.classList.remove('dark')
  })

  describe('初始状态', () => {
    it('模块加载时 localStorage 无值 → 默认深色', async () => {
      localStorage.removeItem(STORAGE_KEY)
      vi.resetModules()
      const fresh = await import('./theme')
      expect(fresh.useThemeStore.getState().theme).toBe('dark')
      vi.resetModules()
    })

    it('模块加载时读取 localStorage 中的 light', async () => {
      localStorage.setItem(STORAGE_KEY, 'light')
      vi.resetModules()
      const fresh = await import('./theme')
      expect(fresh.useThemeStore.getState().theme).toBe('light')
      vi.resetModules()
    })
  })

  describe('toggle', () => {
    it('深色 → 浅色', () => {
      resetStore('dark')
      useThemeStore.getState().toggle()
      expect(useThemeStore.getState().theme).toBe('light')
    })

    it('浅色 → 深色', () => {
      resetStore('light')
      useThemeStore.getState().toggle()
      expect(useThemeStore.getState().theme).toBe('dark')
    })

    it('toggle 后 localStorage 被写入', () => {
      resetStore('dark')
      useThemeStore.getState().toggle()
      expect(localStorage.getItem(STORAGE_KEY)).toBe('light')
    })

    it('toggle 后 html.dark class 被移除', () => {
      document.documentElement.classList.add('dark')
      resetStore('dark')
      useThemeStore.getState().toggle()
      expect(document.documentElement.classList.contains('dark')).toBe(false)
    })
  })

  describe('setTheme', () => {
    it('设置 light', () => {
      resetStore('dark')
      useThemeStore.getState().setTheme('light')
      expect(useThemeStore.getState().theme).toBe('light')
      expect(localStorage.getItem(STORAGE_KEY)).toBe('light')
    })

    it('设置 dark', () => {
      resetStore('light')
      useThemeStore.getState().setTheme('dark')
      expect(useThemeStore.getState().theme).toBe('dark')
      expect(localStorage.getItem(STORAGE_KEY)).toBe('dark')
    })

    it('setTheme 后 html.dark class 被添加', () => {
      resetStore('light')
      useThemeStore.getState().setTheme('dark')
      expect(document.documentElement.classList.contains('dark')).toBe(true)
    })

    it('setTheme 浅色时 html.dark class 被移除', () => {
      document.documentElement.classList.add('dark')
      resetStore('dark')
      useThemeStore.getState().setTheme('light')
      expect(document.documentElement.classList.contains('dark')).toBe(false)
    })
  })

  describe('initTheme', () => {
    it('localStorage 无值时应用深色', () => {
      document.documentElement.classList.remove('dark')
      initTheme()
      expect(document.documentElement.classList.contains('dark')).toBe(true)
      expect(localStorage.getItem(STORAGE_KEY)).toBe('dark')
    })

    it('localStorage 为 light 时移除 dark class', () => {
      localStorage.setItem(STORAGE_KEY, 'light')
      document.documentElement.classList.add('dark')
      initTheme()
      expect(document.documentElement.classList.contains('dark')).toBe(false)
    })
  })

  describe('localStorage 异常回退', () => {
    it('localStorage 抛出异常时回退深色且不抛', () => {
      vi.spyOn(localStorage, 'getItem').mockImplementation(() => {
        throw new Error('blocked')
      })
      vi.spyOn(localStorage, 'setItem').mockImplementation(() => {})
      document.documentElement.classList.remove('dark')
      expect(() => initTheme()).not.toThrow()
      expect(document.documentElement.classList.contains('dark')).toBe(true)
      vi.restoreAllMocks()
    })
  })
})
