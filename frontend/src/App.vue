<template>
  <div class="min-h-screen bg-background">
    <main class="flex flex-col">
      <header class="border-b bg-card px-4 sm:px-6 py-3 sm:py-4">
        <div class="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3 sm:gap-0">
          <div class="flex-1 min-w-0">
            <h1 class="text-lg sm:text-xl md:text-2xl font-bold truncate">MIMO TTS 语音合成</h1>
            <p class="text-xs sm:text-sm text-muted-foreground mt-0.5 sm:mt-1">基于 MIMO v2.5 模型的高品质语音合成服务</p>
          </div>
          
          <div class="flex items-center gap-2 shrink-0">
            <Button 
              variant="outline" 
              size="sm" 
              @click="showTaskSidebar = !showTaskSidebar"
              :class="{ 'bg-primary/10': showTaskSidebar }"
              class="text-xs sm:text-sm"
            >
              <ListIcon class="w-3 h-3 sm:w-4 sm:h-4 mr-1 sm:mr-2" />
              <span class="hidden xs:inline">任务列表</span>
              <span class="xs:hidden">任务</span>
            </Button>
            
            <Button variant="outline" size="sm" @click="showConfigDialog = true" class="text-xs sm:text-sm">
              <SettingsIcon class="w-3 h-3 sm:w-4 sm:h-4 mr-1 sm:mr-2" />
              <span class="hidden xs:inline">配置</span>
            </Button>
            <span class="text-xs text-muted-foreground hidden sm:inline">v2.0</span>
          </div>
        </div>
      </header>

      <div class="flex-1 overflow-y-auto p-3 sm:p-4 md:p-6 space-y-4 sm:space-y-6">
        <SynthesizeForm />
      </div>
    </main>

    <Transition name="slide-in-right">
      <aside 
        ref="sidebarRef"
        v-if="showTaskSidebar" 
        class="fixed right-0 top-0 h-full bg-card border-l shadow-xl z-50 flex flex-col
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

    <div 
      class="fixed right-4 top-1/2 -translate-y-1/2 w-1.5 h-20 xs:h-24 bg-blue-500/60 rounded-l cursor-pointer 
             hover:bg-blue-500/90 hover:w-2 hover:h-28 xs:hover:h-32 transition-all duration-200 z-30
             flex items-center justify-center group shadow-lg"
      @click="showTaskSidebar = true"
      title="点击查看任务列表"
    >
      <svg 
        class="w-2.5 h-2.5 xs:w-3 xs:h-3 text-white opacity-0 group-hover:opacity-100 transition-opacity" 
        fill="none" 
        stroke="currentColor" 
        viewBox="0 0 24 24"
      >
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
      </svg>
    </div>

    <ApiConfigDialog v-model:open="showConfigDialog" />

    <Toaster />
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { useTaskStore } from '@/stores/task'
import { useConfigStore } from '@/stores/config'
import TaskListSidebar from './components/TaskListSidebar.vue'
import SynthesizeForm from './components/SynthesizeForm.vue'
import ApiConfigDialog from './components/ApiConfigDialog.vue'
import { Toaster } from '@/components/ui/sonner'
import { Button } from '@/components/ui/button'
import { 
  Settings as SettingsIcon, 
  List as ListIcon,
  X as XIcon 
} from 'lucide-vue-next'

const taskStore = useTaskStore()
const configStore = useConfigStore()
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
