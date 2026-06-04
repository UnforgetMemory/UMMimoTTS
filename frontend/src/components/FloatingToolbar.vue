<template>
  <div 
    class="fixed top-4 left-1/2 -translate-x-1/2 z-50 flex items-center gap-2 px-3 py-2 rounded-full shadow-2xl dynamic-island"
    role="toolbar"
    aria-label="快捷操作工具栏"
  >
    <!-- 导航标签页 -->
    <template v-for="(tab, idx) in tabs" :key="tab.route">
      <Button 
        variant="ghost" 
        size="sm"
        class="w-auto h-10 px-3 rounded-full text-gray-800 dark:text-white hover:bg-black/10 dark:hover:bg-white/20 transition-all"
        :class="{ 'bg-black/15 dark:bg-white/30': isActive(tab.route) }"
        @click="navigateTo(tab.route)"
        :aria-label="tab.label"
      >
        <component :is="tab.icon" class="w-5 h-5" />
        <span class="text-xs ml-1.5 hidden sm:inline">{{ tab.label }}</span>
      </Button>
      <div v-if="idx < tabs.length - 1" class="w-px h-6 bg-black/15 dark:bg-white/30" />
    </template>

    <!-- API 配置按钮 -->
    <div class="w-px h-6 bg-black/15 dark:bg-white/30" />

    <Button 
      variant="ghost" 
      size="sm"
      class="w-auto h-10 px-2.5 rounded-full text-gray-800 dark:text-white hover:bg-black/10 dark:hover:bg-white/20 transition-all"
      @click="router.push('/config')"
      aria-label="API 配置"
    >
      <KeyIcon class="w-5 h-5" />
      <span class="text-xs ml-1.5 hidden sm:inline">配置</span>
    </Button>

    <!-- 主题切换下拉 -->
    <div class="w-px h-6 bg-black/15 dark:bg-white/30" />

    <DropdownMenu>
      <DropdownMenuTrigger as-child>
        <Button 
          variant="ghost" 
          size="sm"
          class="w-auto h-10 px-2.5 rounded-full text-gray-800 dark:text-white hover:bg-black/10 dark:hover:bg-white/20 transition-all"
          aria-label="切换主题"
        >
          <SunIcon v-if="themeStore.actualTheme === 'light'" class="w-5 h-5" />
          <MoonIcon v-else class="w-5 h-5" />
          <span class="text-xs ml-1.5 hidden sm:inline">主题</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="center">
        <DropdownMenuItem @click="themeStore.setTheme('light')">
          <SunIcon class="w-4 h-4 mr-2" />
          <span>明亮模式</span>
          <DropdownMenuShortcut v-if="themeStore.theme === 'light'">✓</DropdownMenuShortcut>
        </DropdownMenuItem>
        <DropdownMenuItem @click="themeStore.setTheme('dark')">
          <MoonIcon class="w-4 h-4 mr-2" />
          <span>暗色模式</span>
          <DropdownMenuShortcut v-if="themeStore.theme === 'dark'">✓</DropdownMenuShortcut>
        </DropdownMenuItem>
        <DropdownMenuItem @click="themeStore.setTheme('system')">
          <MonitorIcon class="w-4 h-4 mr-2" />
          <span>跟随系统</span>
          <DropdownMenuShortcut v-if="themeStore.theme === 'system'">✓</DropdownMenuShortcut>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  </div>
</template>

<script setup lang="ts">
import { useRouter, useRoute } from 'vue-router'
import { useThemeStore } from '@/stores/theme'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuShortcut,
} from '@/components/ui/dropdown-menu'
import { 
  Key as KeyIcon, 
  Moon as MoonIcon,
  Sun as SunIcon,
  Monitor as MonitorIcon,
  PenSquare as PenSquareIcon,
  List as ListIcon,
  Layers as LayersIcon,
} from 'lucide-vue-next'
import { type Component } from 'vue'

interface Tab {
  label: string
  route: string
  icon: Component
}

const tabs: Tab[] = [
  { label: '合成', route: '/synthesize', icon: PenSquareIcon },
  { label: '单任务', route: '/tasks/single', icon: ListIcon },
  { label: '批量', route: '/tasks/batch', icon: LayersIcon },
]


const router = useRouter()
const route = useRoute()
const themeStore = useThemeStore()

function isActive(tabRoute: string): boolean {
  if (tabRoute === '/synthesize') {
    return route.path === '/synthesize'
  }
  return route.path.startsWith(tabRoute)
}

function navigateTo(path: string) {
  router.push(path)
}
</script>

<style scoped>
.dynamic-island {
  background: rgba(255, 255, 255, 0.15);
  backdrop-filter: blur(20px) saturate(180%);
  -webkit-backdrop-filter: blur(20px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.3);
  box-shadow: 
    0 8px 32px rgba(0, 0, 0, 0.15),
    0 2px 8px rgba(0, 0, 0, 0.1),
    inset 0 1px 0 rgba(255, 255, 255, 0.4);
}

/* 暗色主题下的样式 */
:root.dark .dynamic-island {
  background: rgba(0, 0, 0, 0.5);
  border: 1px solid rgba(255, 255, 255, 0.15);
  box-shadow: 
    0 8px 32px rgba(0, 0, 0, 0.3),
    0 2px 8px rgba(0, 0, 0, 0.2),
    inset 0 1px 0 rgba(255, 255, 255, 0.1);
}
</style>
