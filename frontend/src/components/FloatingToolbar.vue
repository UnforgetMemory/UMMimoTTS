<template>
  <div 
    class="fixed top-4 right-4 z-50 flex flex-col gap-2"
    role="toolbar"
    aria-label="快捷操作工具栏"
  >
    <!-- 主题切换按钮 -->
    <DropdownMenu>
      <DropdownMenuTrigger as-child>
        <Button 
          variant="ghost" 
          size="sm"
          class="w-10 h-10 p-0 rounded-lg hover:bg-muted transition-colors"
          aria-label="切换主题"
        >
          <SunIcon v-if="themeStore.actualTheme === 'light'" class="w-5 h-5" />
          <MoonIcon v-else class="w-5 h-5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
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

    <!-- API 配置按钮 -->
    <Button 
      variant="ghost" 
      size="sm"
      class="w-10 h-10 p-0 rounded-lg hover:bg-muted transition-colors"
      @click="$emit('open-config')"
      aria-label="API 配置"
    >
      <SettingsIcon class="w-5 h-5" />
    </Button>

    <!-- 任务列表按钮 -->
    <Button 
      variant="ghost" 
      size="sm"
      :class="{ 'bg-muted': showTaskSidebar }"
      class="w-10 h-10 p-0 rounded-lg hover:bg-muted transition-colors"
      @click="$emit('toggle-tasks')"
      aria-label="任务列表"
    >
      <ListIcon class="w-5 h-5" />
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
  Settings as SettingsIcon, 
  List as ListIcon,
  Moon as MoonIcon,
  Sun as SunIcon,
  Monitor as MonitorIcon
} from 'lucide-vue-next'

const themeStore = useThemeStore()

defineProps<{
  showTaskSidebar?: boolean
}>()

defineEmits<{
  'open-config': []
  'toggle-tasks': []
}>()
</script>
