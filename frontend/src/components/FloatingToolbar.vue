<template>
  <div 
    class="fixed top-4 left-1/2 -translate-x-1/2 z-50 flex items-center gap-2 px-3 py-2 rounded-full shadow-2xl dynamic-island"
    role="toolbar"
    aria-label="快捷操作工具栏"
  >
    <!-- 批量任务列表按钮（最左边） -->
    <Button 
      variant="ghost" 
      size="sm"
      class="w-auto h-10 px-2.5 rounded-full text-gray-800 dark:text-white hover:bg-black/10 dark:hover:bg-white/20 transition-all"
      :class="{ 'bg-black/15 dark:bg-white/30': showBatchSidebar }"
      @click="$emit('toggle-batch')"
      aria-label="批量任务列表"
    >
      <LayersIcon class="w-5 h-5" />
      <span class="text-xs ml-1.5 hidden sm:inline">任务</span>
    </Button>

    <!-- 分隔线 -->
    <div class="w-px h-6 bg-black/15 dark:bg-white/30" />

    <!-- API 配置按钮 -->
    <Button 
      variant="ghost" 
      size="sm"
      class="w-auto h-10 px-2.5 rounded-full text-gray-800 dark:text-white hover:bg-black/10 dark:hover:bg-white/20 transition-all"
      @click="$emit('open-config')"
      aria-label="API 配置"
    >
      <KeyIcon class="w-5 h-5" />
      <span class="text-xs ml-1.5 hidden sm:inline">配置</span>
    </Button>

    <!-- 分隔线 -->
    <div class="w-px h-6 bg-black/15 dark:bg-white/30" />

    <!-- 主题切换下拉 -->
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

    <!-- 分隔线 -->
    <div class="w-px h-6 bg-black/15 dark:bg-white/30" />

    <!-- 单任务列表按钮（最右边） -->
    <Button 
      variant="ghost" 
      size="sm"
      class="w-auto h-10 px-2.5 rounded-full text-gray-800 dark:text-white hover:bg-black/10 dark:hover:bg-white/20 transition-all"
      :class="{ 'bg-black/15 dark:bg-white/30': showTaskSidebar }"
      @click="$emit('toggle-tasks')"
      aria-label="单任务列表"
    >
      <ListIcon class="w-5 h-5" />
      <span class="text-xs ml-1.5 hidden sm:inline">单任务</span>
    </Button>
  </div>
</template>

<script setup lang="ts">
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
  Layers as LayersIcon,
  List as ListIcon,
  Moon as MoonIcon,
  Sun as SunIcon,
  Monitor as MonitorIcon,
} from 'lucide-vue-next'

defineProps<{
  showBatchSidebar?: boolean
  showTaskSidebar?: boolean
}>()

defineEmits<{
  'open-config': []
  'toggle-batch': []
  'toggle-tasks': []
}>()

const themeStore = useThemeStore()
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
