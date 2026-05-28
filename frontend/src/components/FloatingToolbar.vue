<template>
  <div 
    class="fixed top-4 left-1/2 -translate-x-1/2 z-50 flex items-center gap-2 px-3 py-2 rounded-full shadow-2xl dynamic-island"
    role="toolbar"
    aria-label="快捷操作工具栏"
  >
    <!-- 分组列表切换按钮 -->
    <Button 
      variant="ghost" 
      size="sm"
      class="w-10 h-10 p-0 rounded-full text-white hover:text-white hover:bg-white/20 transition-all"
      :class="{ 'bg-white/30': showTaskSidebar }"
      @click="$emit('toggle-tasks')"
      aria-label="分组列表"
    >
      <LayersIcon class="w-5 h-5" />
    </Button>

    <!-- 分隔线 -->
    <div class="w-px h-6 bg-white/30" />

    <!-- API 配置按钮 -->
    <Button 
      variant="ghost" 
      size="sm"
      class="w-10 h-10 p-0 rounded-full text-white hover:text-white hover:bg-white/20 transition-all"
      @click="$emit('open-config')"
      aria-label="API 配置"
    >
      <KeyIcon class="w-5 h-5" />
    </Button>

    <!-- 分隔线 -->
    <div class="w-px h-6 bg-white/30" />

    <!-- 主题切换下拉 -->
    <DropdownMenu>
      <DropdownMenuTrigger as-child>
        <Button 
          variant="ghost" 
          size="sm"
          class="w-10 h-10 p-0 rounded-full text-white hover:text-white hover:bg-white/20 transition-all"
          aria-label="切换主题"
        >
          <SunIcon v-if="themeStore.actualTheme === 'light'" class="w-5 h-5" />
          <MoonIcon v-else class="w-5 h-5" />
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
    <div class="w-px h-6 bg-white/30" />

    <!-- 语言切换下拉 -->
    <DropdownMenu>
      <DropdownMenuTrigger as-child>
        <Button 
          variant="ghost" 
          size="sm"
          class="w-10 h-10 p-0 rounded-full text-white hover:text-white hover:bg-white/20 transition-all"
          aria-label="切换语言"
        >
          <GlobeIcon class="w-5 h-5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="center">
        <DropdownMenuItem @click="setLocale('zh-CN')">
          <span>🇨🇳</span>
          <span class="ml-2">简体中文</span>
          <DropdownMenuShortcut v-if="locale === 'zh-CN'">✓</DropdownMenuShortcut>
        </DropdownMenuItem>
        <DropdownMenuItem @click="setLocale('en')">
          <span>🇺🇸</span>
          <span class="ml-2">English</span>
          <DropdownMenuShortcut v-if="locale === 'en'">✓</DropdownMenuShortcut>
        </DropdownMenuItem>
        <DropdownMenuItem @click="setLocale('ja')">
          <span>🇯🇵</span>
          <span class="ml-2">日本語</span>
          <DropdownMenuShortcut v-if="locale === 'ja'">✓</DropdownMenuShortcut>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
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
  Moon as MoonIcon,
  Sun as SunIcon,
  Monitor as MonitorIcon,
  Globe as GlobeIcon
} from 'lucide-vue-next'

defineProps<{
  showTaskSidebar?: boolean
}>()

defineEmits<{
  'open-config': []
  'toggle-tasks': []
}>()

const themeStore = useThemeStore()
const locale = ref(localStorage.getItem('locale') || 'zh-CN')

function setLocale(lang: string) {
  locale.value = lang
  localStorage.setItem('locale', lang)
  // TODO: 实际切换语言需要集成 i18n
  window.location.reload()
}
</script>

<style scoped>
.dynamic-island {
  background: rgba(0, 0, 0, 0.85);
  backdrop-filter: blur(20px) saturate(180%);
  -webkit-backdrop-filter: blur(20px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.2);
  box-shadow: 
    0 8px 32px rgba(0, 0, 0, 0.4),
    0 2px 8px rgba(0, 0, 0, 0.2),
    inset 0 1px 0 rgba(255, 255, 255, 0.15);
}
</style>
