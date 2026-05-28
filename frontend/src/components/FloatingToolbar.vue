<template>
  <div 
    class="fixed top-3 left-1/2 -translate-x-1/2 z-50 flex items-center gap-1 px-2 py-1.5 bg-foreground/90 backdrop-blur-xl rounded-full shadow-lg border border-foreground/20"
    role="toolbar"
    aria-label="快捷操作工具栏"
  >
    <!-- 分组列表切换按钮 -->
    <Button 
      variant="ghost" 
      size="sm"
      class="w-8 h-8 p-0 rounded-full text-background hover:text-background hover:bg-background/20 transition-all"
      :class="{ 'bg-background/30': showTaskSidebar }"
      @click="$emit('toggle-tasks')"
      aria-label="分组列表"
    >
      <LayersIcon class="w-4 h-4" />
    </Button>

    <!-- 分隔线 -->
    <div class="w-px h-5 bg-background/30 mx-0.5" />

    <!-- API 配置按钮 -->
    <Button 
      variant="ghost" 
      size="sm"
      class="w-8 h-8 p-0 rounded-full text-background hover:text-background hover:bg-background/20 transition-all"
      @click="$emit('open-config')"
      aria-label="API 配置"
    >
      <KeyIcon class="w-4 h-4" />
    </Button>

    <!-- 分隔线 -->
    <div class="w-px h-5 bg-background/30 mx-0.5" />

    <!-- 主题切换下拉 -->
    <DropdownMenu>
      <DropdownMenuTrigger as-child>
        <Button 
          variant="ghost" 
          size="sm"
          class="w-8 h-8 p-0 rounded-full text-background hover:text-background hover:bg-background/20 transition-all"
          aria-label="切换主题"
        >
          <SunIcon v-if="themeStore.actualTheme === 'light'" class="w-4 h-4" />
          <MoonIcon v-else class="w-4 h-4" />
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
    <div class="w-px h-5 bg-background/30 mx-0.5" />

    <!-- 语言切换下拉 -->
    <DropdownMenu>
      <DropdownMenuTrigger as-child>
        <Button 
          variant="ghost" 
          size="sm"
          class="w-8 h-8 p-0 rounded-full text-background hover:text-background hover:bg-background/20 transition-all"
          aria-label="切换语言"
        >
          <GlobeIcon class="w-4 h-4" />
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
