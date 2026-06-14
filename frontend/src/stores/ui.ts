import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

export const useUIStore = defineStore('ui', () => {
  const sidebarOpen = ref(false)
  const sidebarCollapsed = ref(false)
  const theme = ref<'light' | 'dark' | 'system'>('system')

  const isDark = computed(() => {
    if (theme.value === 'dark') return true
    if (theme.value === 'light') return false
    return window.matchMedia('(prefers-color-scheme: dark)').matches
  })

  function toggleSidebar() { sidebarOpen.value = !sidebarOpen.value }
  function collapseSidebar() { sidebarCollapsed.value = true }
  function expandSidebar() { sidebarCollapsed.value = false }
  function setTheme(t: 'light' | 'dark' | 'system') { theme.value = t }

  return { sidebarOpen, sidebarCollapsed, theme, isDark, toggleSidebar, collapseSidebar, expandSidebar, setTheme }
})
