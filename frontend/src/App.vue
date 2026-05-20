<template>
  <div class="min-h-screen bg-background relative">
    <!-- 背景水印 -->
    <BrandHero />
    
    <!-- 主内容区 -->
    <main class="relative z-10 flex flex-col items-center justify-start min-h-screen px-4 py-8 sm:py-12">
      
      <!-- 合成表单 -->
      <div class="w-full max-w-4xl mt-8 sm:mt-12">
        <SynthesizeForm />
      </div>
      
      <!-- 底部信息 -->
      <FooterInfo />
    </main>

    <!-- 悬浮工具栏 -->
    <FloatingToolbar 
      :show-task-sidebar="showTaskSidebar"
      @open-config="showConfigDialog = true"
      @toggle-tasks="showTaskSidebar = !showTaskSidebar"
    />

    <!-- 任务列表面板（保持现有逻辑） -->
    <Transition name="slide-in-right">
      <aside 
        ref="sidebarRef"
        v-if="showTaskSidebar" 
        class="fixed right-0 top-0 h-full bg-background text-foreground border-l shadow-xl z-50 flex flex-col
               w-full xs:w-80 sm:w-96 md:w-[28rem] lg:w-[32rem]"
        role="complementary"
        aria-label="任务列表面板"
        aria-modal="true"
        tabindex="-1"
      >
        <div class="p-3 sm:p-4 border-b flex items-center justify-between">
          <div class="min-w-0 flex-1">
            <h2 class="text-base sm:text-lg font-semibold truncate">任务列表</h2>
            <p class="text-xs text-muted-foreground mt-0.5 sm:mt-1">查看和管理合成任务</p>
          </div>
          <Button 
            variant="ghost" 
            size="sm" 
            class="h-7 w-7 sm:h-8 sm:w-8 p-0 shrink-0 ml-2"
            @click="showTaskSidebar = false"
          >
            <XIcon class="w-3.5 h-3.5 sm:w-4 sm:h-4" />
          </Button>
        </div>
        
        <div class="flex-1 overflow-y-auto p-3 sm:p-4">
          <TaskListSidebar />
        </div>
      </aside>
    </Transition>

    <Transition name="fade">
      <div 
        v-if="showTaskSidebar"
        class="fixed inset-0 bg-black/20 z-40"
        @click="showTaskSidebar = false"
        aria-hidden="true"
      ></div>
    </Transition>

    <ApiConfigDialog v-model:open="showConfigDialog" />

    <Toaster />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { useTaskStore } from '@/stores/task'
import { useConfigStore } from '@/stores/config'
import { useThemeStore } from '@/stores/theme'
import BrandHero from './components/BrandHero.vue'
import FloatingToolbar from './components/FloatingToolbar.vue'
import FooterInfo from './components/FooterInfo.vue'
import TaskListSidebar from './components/TaskListSidebar.vue'
import SynthesizeForm from './components/SynthesizeForm.vue'
import ApiConfigDialog from './components/ApiConfigDialog.vue'
import { Toaster } from '@/components/ui/sonner'
import { Button } from '@/components/ui/button'
import { X as XIcon } from 'lucide-vue-next'

const taskStore = useTaskStore()
const configStore = useConfigStore()
const themeStore = useThemeStore()
const showConfigDialog = ref(false)
const showTaskSidebar = ref(false)
const sidebarRef = ref<HTMLElement | null>(null)

// 键盘事件处理 - ESC 键关闭侧边栏
function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && showTaskSidebar.value) {
    showTaskSidebar.value = false
  }
}

// 监听侧边栏状态变化，管理焦点
watch(showTaskSidebar, async (newValue) => {
  if (newValue) {
    await nextTick()
    sidebarRef.value?.focus()
  }
})

onMounted(() => {
  // 初始化主题
  themeStore.init()
  
  configStore.loadFromStorage()
  taskStore.init()
  // 添加键盘监听
  window.addEventListener('keydown', handleKeydown)
})

onUnmounted(() => {
  taskStore.cleanup()
  // 移除键盘监听
  window.removeEventListener('keydown', handleKeydown)
})
</script>

<style scoped>
/* Slide in from right animation */
.slide-in-right-enter-active,
.slide-in-right-leave-active {
  transition: transform 0.3s ease;
}

.slide-in-right-enter-from {
  transform: translateX(100%);
}

.slide-in-right-leave-to {
  transform: translateX(100%);
}

/* Fade animation for overlay */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
