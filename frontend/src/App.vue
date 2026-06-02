<template>
  <div class="min-h-screen bg-background relative">
    <!-- 背景水印 -->
    <BrandHero />
    
    <!-- 主内容区 -->
    <div class="relative z-1 flex min-h-screen">
      <!-- 左侧批量任务面板 (覆盖式，不占位) -->
      <Transition name="slide-in-left">
        <aside
          v-if="!sidebarCollapsed"
          class="fixed left-0 top-0 h-full bg-background text-foreground border-r shadow-lg z-50 flex flex-col
                 w-full xs:w-80 sm:w-96 md:w-[28rem] lg:w-[30rem]"
          role="complementary"
          aria-label="批量任务面板"
          aria-modal="true"
          tabindex="-1"
        >
          <!-- Header -->
          <div class="px-4 sm:px-5 py-3.5 border-b shrink-0">
            <div class="flex items-center justify-between">
              <div class="min-w-0 flex-1">
                <h2 class="text-base sm:text-lg font-semibold tracking-tight text-foreground">批量任务</h2>
                <p class="text-xs text-muted-foreground/70 mt-0.5">管理批量合成分组</p>
              </div>
              <Button 
                variant="ghost" 
                size="sm" 
                class="h-7 w-7 sm:h-8 sm:w-8 p-0 shrink-0 ml-2 text-muted-foreground hover:text-foreground"
                @click="sidebarCollapsed = true"
              >
                <XIcon class="w-3.5 h-3.5 sm:w-4 sm:h-4" />
              </Button>
            </div>
          </div>
          
          <!-- New task button -->
          <div class="px-4 sm:px-5 py-3 border-b">
            <Button
              class="w-full h-9 text-xs"
              size="sm"
              :disabled="!configStore.hasValidKey"
              @click="showBatchWizard = true"
            >
              <PlusIcon class="w-4 h-4 mr-1.5" />
              {{ configStore.hasValidKey ? '新建批量任务' : '请先配置 API Key' }}
            </Button>
          </div>

          <!-- Group count + clear all -->
          <div class="flex items-center justify-between px-4 sm:px-5 py-2 border-b bg-muted/10">
            <span class="text-xs font-medium text-muted-foreground">
              {{ batchStore.groups.length }} 个分组
            </span>
            <Button
              v-if="batchStore.groups.length > 0"
              variant="ghost"
              size="sm"
              class="h-7 text-xs text-muted-foreground hover:text-destructive px-2 -mr-1"
              @click="handleClearAll"
            >
              <Trash2Icon class="w-3.5 h-3.5 mr-1" />
              一键清空
            </Button>
          </div>

          <!-- Group list -->
          <div class="flex-1 overflow-hidden">
            <GroupKanban
              :groups="batchStore.groups"
              :selected-group-id="selectedGroupId"
              :loading="batchStore.loading"
              @select="selectedGroupId = $event"
              @pause="handlePauseGroup"
              @resume="handleResumeGroup"
              @retry="handleRetryGroup"
              @cancel="handleCancelGroup"
              @delete="handleDeleteGroup"
              @download="handleDownloadGroup"
              class="h-full"
            />
          </div>
        </aside>
      </Transition>

      <!-- Overlay -->
      <Transition name="fade">
        <div 
          v-if="!sidebarCollapsed"
          class="fixed inset-0 bg-black/15 z-40"
          @click="sidebarCollapsed = true"
        />
      </Transition>

      <!-- Center content area -->
      <main class="flex-1 flex flex-col items-center justify-start px-4 py-6 sm:py-10 overflow-y-auto scrollbar-auto">
        <!-- Selected group detail or synthesize form -->
        <template v-if="selectedGroup">
          <div class="w-full h-full">
            <GroupDetailPanel
              :group="selectedGroup"
              :downloading="batchStore.downloadingGroupId === selectedGroup.id"
              @close="selectedGroupId = null"
              @pause="handlePauseGroup"
              @resume="handleResumeGroup"
              @retry="handleRetryGroup"
              @download="handleDownloadGroup"
              @play="handleOpenPlayer"
              @view-text="handleOpenTextViewer"
            />
          </div>
        </template>
        <div v-else class="w-full max-w-4xl mt-6 sm:mt-10">
          <SynthesizeForm ref="synthesizeFormRef" @submitted="showTaskSidebar = true" />
        </div>
        
        <!-- Footer -->
        <FooterInfo />
      </main>
    </div>

    <!-- Floating toolbar -->
    <FloatingToolbar 
      :show-batch-sidebar="!sidebarCollapsed"
      :show-task-sidebar="showTaskSidebar"
      @open-config="showConfigDialog = true"
      @toggle-batch="sidebarCollapsed = !sidebarCollapsed"
      @toggle-tasks="showTaskSidebar = !showTaskSidebar"
      class="z-50"
    />

    <!-- Right task list panel -->
    <Transition name="slide-in-right">
      <aside 
        ref="sidebarRef"
        v-if="showTaskSidebar" 
        class="fixed right-0 top-0 h-full bg-background text-foreground border-l shadow-lg z-50 flex flex-col
               w-full xs:w-80 sm:w-96 md:w-[28rem] lg:w-[30rem]"
        role="complementary"
        aria-label="任务列表面板"
        aria-modal="true"
        tabindex="-1"
      >
        <div class="px-4 sm:px-5 py-3.5 border-b shrink-0 flex items-center justify-between">
          <div class="min-w-0 flex-1">
            <h2 class="text-base sm:text-lg font-semibold tracking-tight text-foreground">任务列表</h2>
            <p class="text-xs text-muted-foreground/70 mt-0.5">查看和管理合成任务</p>
          </div>
          <Button 
            variant="ghost" 
            size="sm" 
            class="h-7 w-7 sm:h-8 sm:w-8 p-0 shrink-0 ml-2 text-muted-foreground hover:text-foreground"
            @click="showTaskSidebar = false"
          >
            <XIcon class="w-3.5 h-3.5 sm:w-4 sm:h-4" />
          </Button>
        </div>
        
        <!-- Task count + clear all -->
        <div class="flex items-center justify-between px-4 sm:px-5 py-2 border-b bg-muted/10">
          <span class="text-xs font-medium text-muted-foreground">
            {{ taskStore.standaloneTasks.length }} 个任务
          </span>
          <Button
            v-if="taskStore.standaloneTasks.length > 0"
            variant="ghost"
            size="sm"
            class="h-7 text-xs text-muted-foreground hover:text-destructive px-2 -mr-1"
            @click="handleClearAllTasks"
          >
            <Trash2Icon class="w-3.5 h-3.5 mr-1" />
            一键清空
          </Button>
        </div>
        
        <div class="flex-1 overflow-hidden">
          <TaskListSidebar
            class="h-full"
            @open-player="handleOpenPlayer"
            @reuse-config="handleReuseConfig"
            @open-text-viewer="handleOpenTextViewer"
          />
        </div>
      </aside>
    </Transition>

    <Transition name="fade">
      <div 
        v-if="showTaskSidebar"
        class="fixed inset-0 bg-black/15 z-40"
        @click="showTaskSidebar = false"
        aria-hidden="true"
      ></div>
    </Transition>

    <!-- Mobile batch sidebar -->
    <Transition name="slide-in-left">
      <aside
        v-if="showMobileBatchSidebar"
        class="fixed left-0 top-0 h-full w-72 bg-background border-r shadow-lg z-50 flex flex-col lg:hidden"
      >
        <div class="px-4 py-3.5 border-b flex items-center justify-between">
          <h2 class="text-sm font-semibold text-foreground">批量任务</h2>
          <Button
            variant="ghost"
            size="sm"
            class="h-7 w-7 p-0 text-muted-foreground hover:text-foreground"
            @click="showMobileBatchSidebar = false"
          >
            <XIcon class="w-4 h-4" />
          </Button>
        </div>
        <div class="flex-1 overflow-y-auto scrollbar-auto p-3 space-y-2">
          <Button
            class="w-full h-9 text-xs"
            size="sm"
            @click="showMobileBatchSidebar = false; showBatchWizard = true"
          >
            <PlusIcon class="w-4 h-4 mr-1.5" />
            新建批量任务
          </Button>
          <GroupCard
            v-for="group in batchStore.groups"
            :key="group.id"
            :group="group"
            :selected="selectedGroupId === group.id"
            @select="selectedGroupId = $event; showMobileBatchSidebar = false"
            @pause="handlePauseGroup"
            @resume="handleResumeGroup"
            @retry="handleRetryGroup"
            @cancel="handleCancelGroup"
            @delete="handleDeleteGroup"
          />
        </div>
      </aside>
    </Transition>

    <Transition name="fade">
      <div
        v-if="showMobileBatchSidebar"
        class="fixed inset-0 bg-black/15 z-40 lg:hidden"
        @click="showMobileBatchSidebar = false"
        aria-hidden="true"
      ></div>
    </Transition>

    <ApiConfigDialog v-model:open="showConfigDialog" />
    
    <AudioPlayerDialog
      v-model:open="showAudioPlayer"
      :task-id="currentAudioTaskId"
      :original-text="getCurrentTaskText()"
    />

    <TextViewerDialog
      v-model:open="showTextDialog"
      :task="currentTextTask"
    />

    <BatchImportWizard
      v-model:open="showBatchWizard"
      @imported="handleBatchImported"
    />

    <Toaster />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { useTaskStore } from '@/stores/task'
import { useBatchStore } from '@/stores/batch'
import { useConfigStore } from '@/stores/config'
import type { Task, TaskSummary } from '@/api/client'
import { toast } from 'vue-sonner'
import BrandHero from './components/BrandHero.vue'
import FloatingToolbar from './components/FloatingToolbar.vue'
import FooterInfo from './components/FooterInfo.vue'
import TaskListSidebar from './components/TaskListSidebar.vue'
import SynthesizeForm from './components/SynthesizeForm.vue'
import ApiConfigDialog from './components/ApiConfigDialog.vue'
import AudioPlayerDialog from './components/AudioPlayerDialog.vue'
import TextViewerDialog from './components/TextViewerDialog.vue'
import BatchImportWizard from './components/BatchImportWizard.vue'
import GroupKanban from './components/GroupKanban.vue'
import GroupCard from './components/GroupCard.vue'
import GroupDetailPanel from './components/GroupDetailPanel.vue'
import { Toaster } from '@/components/ui/sonner'
import { Button } from '@/components/ui/button'
import { 
  X as XIcon, 
  Plus as PlusIcon,
  Trash2 as Trash2Icon,
} from 'lucide-vue-next'

const taskStore = useTaskStore()
const batchStore = useBatchStore()
const configStore = useConfigStore()
const showConfigDialog = ref(false)
const showTaskSidebar = ref(false)
const showBatchWizard = ref(false)
const showMobileBatchSidebar = ref(false)
const sidebarCollapsed = ref(true)  // 默认折叠
const sidebarRef = ref<HTMLElement | null>(null)
const showAudioPlayer = ref(false)
const currentAudioTaskId = ref<string | null>(null)
const currentAudioTaskText = ref('')
const showTextDialog = ref(false)
const currentTextTask = ref<Task | null>(null)
const synthesizeFormRef = ref<InstanceType<typeof SynthesizeForm> | null>(null)
const selectedGroupId = ref<string | null>(null)

const selectedGroup = computed(() => {
  if (!selectedGroupId.value) return null
  return batchStore.groups.find(g => g.id === selectedGroupId.value) || null
})

// Keyboard event - ESC to close sidebars
function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    if (showTaskSidebar.value) {
      showTaskSidebar.value = false
    } else if (!sidebarCollapsed.value) {
      sidebarCollapsed.value = true
    }
  }
}

watch(showTaskSidebar, async (newValue) => {
  if (newValue) {
    await nextTick()
    sidebarRef.value?.focus()
  }
})

// Audio player handlers
async function handleOpenPlayer(task: Task | TaskSummary) {
  currentAudioTaskId.value = task.id
  if ('text' in (task as Task) && typeof (task as Task).text === 'string') {
    currentAudioTaskText.value = (task as Task).text || ''
  } else {
    try {
      const full = await taskStore.getTaskDetail(task.id)
      currentAudioTaskText.value = full.text || ''
    } catch {
      currentAudioTaskText.value = ''
    }
  }
  showAudioPlayer.value = true
}

function handleReuseConfig(config: { text: string; voice: string | null; model: string; context?: string }) {
  synthesizeFormRef.value?.setConfig(config)
  showTaskSidebar.value = false
}

async function handleOpenTextViewer(task: Task | TaskSummary) {
  if ('text' in (task as Task) && typeof (task as Task).text === 'string') {
    currentTextTask.value = task as Task
  } else {
    try {
      const full = await taskStore.getTaskDetail(task.id)
      currentTextTask.value = full
    } catch {
      currentTextTask.value = null
      return
    }
  }
  showTextDialog.value = true
}

function getCurrentTaskText(): string {
  return currentAudioTaskText.value
}

// Batch group handlers
async function handlePauseGroup(groupId: string) {
  try {
    await batchStore.pauseGroup(groupId)
    toast.success('分组已暂停')
  } catch (error) {
    toast.error('暂停失败')
  }
}

async function handleResumeGroup(groupId: string) {
  try {
    await batchStore.resumeGroup(groupId)
    toast.success('分组已恢复')
  } catch (error) {
    toast.error('恢复失败')
  }
}

async function handleRetryGroup(groupId: string) {
  try {
    await batchStore.retryFailed(groupId)
    toast.success('失败任务已重新排队')
  } catch (error) {
    toast.error('重试失败')
  }
}

async function handleDeleteGroup(groupId: string) {
  if (!confirm('确定删除此分组及其所有任务？')) return
  try {
    await batchStore.removeGroup(groupId)
    if (selectedGroupId.value === groupId) {
      selectedGroupId.value = null
    }
    toast.success('分组已删除')
  } catch (error) {
    toast.error('删除失败')
  }
}

async function handleCancelGroup(groupId: string) {
  try {
    await batchStore.cancelGroup(groupId)
    toast.success('分组已停止')
  } catch (error) {
    toast.error('停止失败')
  }
}

async function handleClearAll() {
  const count = batchStore.groups.length
  if (!confirm(`确定要清空全部 ${count} 个分组吗？此操作不可恢复。`)) return
  try {
    await batchStore.clearAll()
    selectedGroupId.value = null
    toast.success('已清空全部分组')
  } catch (error) {
    toast.error('清空失败')
  }
}

async function handleClearAllTasks() {
  const count = taskStore.standaloneTasks.length
  if (!confirm(`确定要清空全部 ${count} 个任务吗？此操作不可恢复。`)) return
  try {
    await taskStore.clearAll()
    toast.success('已清空全部任务')
  } catch (error) {
    toast.error('清空失败')
  }
}

async function handleDownloadGroup(groupId: string) {
  try {
    await batchStore.downloadGroupAudio(groupId)
    toast.success('下载已开始')
  } catch (error) {
    toast.error('下载失败')
  }
}

async function handleBatchImported(groupId: string) {
  selectedGroupId.value = groupId
  showBatchWizard.value = false
  await Promise.all([
    batchStore.loadGroups(),
    taskStore.loadTasks(),
  ])
}

// Lifecycle
onMounted(async () => {
  document.addEventListener('keydown', handleKeydown)
  // init() calls loadTasks() + restoreSseSubscriptions() + startPolling() (30s fallback)
  taskStore.init()
  await batchStore.loadGroups()
})

onUnmounted(() => {
  document.removeEventListener('keydown', handleKeydown)
})
</script>

<style>
.slide-in-right-enter-active,
.slide-in-right-leave-active {
  transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}
.slide-in-right-enter-from,
.slide-in-right-leave-to {
  transform: translateX(100%);
}

.slide-in-left-enter-active,
.slide-in-left-leave-active {
  transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}
.slide-in-left-enter-from,
.slide-in-left-leave-to {
  transform: translateX(-100%);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
