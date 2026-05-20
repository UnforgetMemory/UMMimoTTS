import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useThemeStore = defineStore('theme', () => {
  // 主题类型：'light' | 'dark' | 'system'
  const theme = ref<'light' | 'dark' | 'system'>('system')
  
  // 当前实际生效的主题（用于判断是否应用 .dark 类）
  const actualTheme = ref<'light' | 'dark'>('light')
  
  // 初始化：从 localStorage 读取或默认为 system
  function init() {
    const saved = localStorage.getItem('theme') as 'light' | 'dark' | 'system' | null
    if (saved) {
      theme.value = saved
    }
    updateActualTheme()
  }
  
  // 根据 theme 值和系统偏好更新 actualTheme
  function updateActualTheme() {
    if (theme.value === 'system') {
      // 检测系统偏好
      const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
      actualTheme.value = prefersDark ? 'dark' : 'light'
    } else {
      actualTheme.value = theme.value
    }
    
    // 应用 .dark 类到 html 元素
    applyThemeClass()
  }
  
  // 应用主题类到 DOM
  function applyThemeClass() {
    const html = document.documentElement
    if (actualTheme.value === 'dark') {
      html.classList.add('dark')
    } else {
      html.classList.remove('dark')
    }
  }
  
  // 切换主题
  function setTheme(newTheme: 'light' | 'dark' | 'system') {
    theme.value = newTheme
    localStorage.setItem('theme', newTheme)
    updateActualTheme()
  }
  
  // 监听系统主题变化
  function setupSystemThemeListener() {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    
    const handleChange = () => {
      if (theme.value === 'system') {
        updateActualTheme()
      }
    }
    
    // 现代浏览器使用 addEventListener
    if (mediaQuery.addEventListener) {
      mediaQuery.addEventListener('change', handleChange)
    } else {
      // 兼容旧浏览器
      mediaQuery.addListener(handleChange)
    }
  }
  
  // 初始化时设置监听器
  init()
  setupSystemThemeListener()
  
  return {
    theme,
    actualTheme,
    setTheme,
    init,
  }
})
