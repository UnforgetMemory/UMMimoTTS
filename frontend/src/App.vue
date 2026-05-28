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
          class="fixed left-0 top-0 h-full bg-background text-foreground border-r shadow-xl z-50 flex flex-col
                 w-full xs:w-80 sm:w-96 md:w-[28rem] lg:w-[32rem]"
          role="complementary"
          aria-label="批量任务面板"
          aria-modal="true"
          tabindex="-1"
        >
          <div class="p-3 sm:p-4 border-b flex items-center justify-between">
            <div class="min-w-0 flex-1">
              <h2 class="text-base sm:text-lg font-semibold truncate">批量任务</h2>
              <p class="text-xs text-muted-foreground mt-0.5 sm:mt-1">管理批量合成分组</p>
            </div>
            <Button 
              variant="ghost" 
              size="sm" 
              class="h-7 w-7 sm:h-8 sm:w-8 p-0 shrink-0 ml-2"
              @click="sidebarCollapsed = true"
            >
              <XIcon class="w-3.5 h-3.5 sm:w-4 sm:h-4" />
            </Button>
          </div>
          
          <div class="p-3 sm:p-4 border-b">
            <Button
              class="w-full"
              size="sm"
              :disabled="!configStore.hasValidKey"
              @click="showBatchWizard = true"
            >
              <PlusIcon class="w-4 h-4 mr-1" />
              {{ configStore.hasValidKey ? '新建批量任务' : '请先配置 API Key' }}
            </Button>
          </div>

          <div class="flex-1 overflow-hidden">
            <div class="h-full overflow-y-auto p-3 sm:p-4 space-y-2">
              <div
                v-for="group in batchStore.groups"
                :key="group.id"
                class="group relative p-2.5 sm:p-3 rounded-lg border cursor-pointer transition-all hover:shadow-sm"
                :class="selectedGroupId === group.id ? 'bg-primary/5 border-primary/20 shadow-sm' : 'bg-card hover:bg-muted/50'"
                @click="selectedGroupId = group.id"
              >
                <div class="flex items-start justify-between">
                  <div class="flex-1 min-w-0">
                    <h3 class="font-medium text-xs sm:text-sm truncate">{{ group.name }}</h3>
                    <div class="flex items-center gap-1.5 sm:gap-2 mt-1">
                      <Badge :variant="getStatusBadge(group.status).variant" class="text-[10px] sm:text-xs px-1 sm:px-1.5">
                        {{ getStatusBadge(group.status).label }}
                      </Badge>
                      <span class="text-[10px] sm:text-xs text-muted-foreground">
                        {{ group.completed_tasks }}/{{ group.task_count }}
                      </span>
                    </div>
                  </div>
                  <div class="flex items-center gap-0.5 sm:gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                    <Button
                      v-if="group.status === 'processing'"
                      variant="ghost"
                      size="icon-sm"
                      class="h-5 w-5 sm:h-6 sm:w-6"
                      @click.stop="pauseGroup(group.id)"
                      :disabled="batchStore.loading"
                    >
                      <PauseIcon class="w-2.5 h-2.5 sm:w-3 sm:h-3" />
                    </Button>
                    <Button
                      v-if="group.status === 'paused'"
                      variant="ghost"
                      size="icon-sm"
                      class="h-5 w-5 sm:h-6 sm:w-6"
                      @click.stop="resumeGroup(group.id)"
                      :disabled="batchStore.loading"
                    >
                      <PlayIcon class="w-2.5 h-2.5 sm:w-3 sm:h-3" />
                    </Button>
                    <Button
                      v-if="group.status === 'completed' && group.completed_tasks > 0"
                      variant="ghost"
                      size="icon-sm"
                      class="h-5 w-5 sm:h-6 sm:w-6"
                      @click.stop="downloadGroup(group.id)"
                    >
                      <DownloadIcon class="w-2.5 h-2.5 sm:w-3 sm:h-3" />
                    </Button>
                    <Button
                      v-if="group.status === 'failed'"
                      variant="ghost"
                      size="icon-sm"
                      class="h-5 w-5 sm:h-6 sm:w-6"
                      @click.stop="retryGroup(group.id)"
                      :disabled="batchStore.loading"
                    >
                      <RotateCcwIcon class="w-2.5 h-2.5 sm:w-3 sm:h-3" />
                    </Button>
                    <Button
                      v-if="group.status !== 'processing'"
                      variant="ghost"
                      size="icon-sm"
                      class="h-5 w-5 sm:h-6 sm:w-6 text-destructive hover:text-destructive"
                      @click.stop="deleteGroup(group.id)"
                      :disabled="batchStore.loading"
                    >
                      <TrashIcon class="w-2.5 h-2.5 sm:w-3 sm:h-3" />
                    </Button>
                  </div>
                </div>
                
                <div v-if="group.status === 'processing' || group.status === 'chunking'" class="mt-2">
                  <Progress :model-value="group.progress" class="h-1" />
                </div>
                
                <div class="flex items-center gap-1 mt-1.5 text-[10px] text-muted-foreground">
                  <ClockIcon class="w-2.5 h-2.5" />
                  <span>{{ formatRelativeTime(group.created_at) }}</span>
                </div>
              </div>

              <div v-if="batchStore.groups.length === 0" class="text-center py-8">
                <FolderIcon class="w-10 h-10 mx-auto text-muted-foreground/50 mb-3" />
                <p class="text-sm text-muted-foreground">暂无批量任务</p>
                <p class="text-xs text-muted-foreground/70 mt-1">点击上方按钮创建</p>
              </div>
            </div>
          </div>
        </aside>
      </Transition>

      <!-- 遮罩层 -->
      <Transition name="fade">
        <div 
          v-if="!sidebarCollapsed"
          class="fixed inset-0 bg-black/20 z-40"
          @click="sidebarCollapsed = true"
        />
      </Transition>

      <!-- 中心内容区 -->
      <main class="flex-1 flex flex-col items-center justify-start px-4 py-8 sm:py-12 overflow-y-auto scrollbar-auto">
        <!-- 选中分组详情 或 合成表单 -->
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
        <div v-else class="w-full max-w-4xl mt-8 sm:mt-12">
          <SynthesizeForm ref="synthesizeFormRef" />
        </div>
        
        <!-- 底部信息 -->
        <FooterInfo />
      </main>
    </div>

    <!-- 悬浮工具栏 -->
    <FloatingToolbar 
      :show-batch-sidebar="!sidebarCollapsed"
      :show-task-sidebar="showTaskSidebar"
      @open-config="showConfigDialog = true"
      @toggle-batch="sidebarCollapsed = !sidebarCollapsed"
      @toggle-tasks="showTaskSidebar = !showTaskSidebar"
      class="z-50"
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
        class="fixed inset-0 bg-black/20 z-40"
        @click="showTaskSidebar = false"
        aria-hidden="true"
      ></div>
    </Transition>

    <!-- 移动端批量任务面板 -->
    <Transition name="slide-in-left">
      <aside
        v-if="showMobileBatchSidebar"
        class="fixed left-0 top-0 h-full w-72 bg-background border-r shadow-xl z-50 flex flex-col lg:hidden"
      >
        <div class="p-4 border-b flex items-center justify-between">
          <h2 class="text-base font-semibold">批量任务</h2>
          <Button
            variant="ghost"
            size="sm"
            class="h-7 w-7 p-0"
            @click="showMobileBatchSidebar = false"
          >
            <XIcon class="w-4 h-4" />
          </Button>
        </div>
        <div class="flex-1 overflow-y-auto scrollbar-auto p-3 space-y-2">
          <Button
            class="w-full"
            size="sm"
            @click="showMobileBatchSidebar = false; showBatchWizard = true"
          >
            <PlusIcon class="w-4 h-4 mr-1" />
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
            @delete="handleDeleteGroup"
          />
        </div>
      </aside>
    </Transition>

    <Transition name="fade">
      <div
        v-if="showMobileBatchSidebar"
        class="fixed inset-0 bg-black/20 z-40 lg:hidden"
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
import GroupCard from './components/GroupCard.vue'
import GroupDetailPanel from './components/GroupDetailPanel.vue'
import { Toaster } from '@/components/ui/sonner'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import { Skeleton } from '@/components/ui/skeleton'
import { Separator } from '@/components/ui/separator'
import { 
  X as XIcon, 
  Plus as PlusIcon, 
  Layers as LayersIcon, 
  PanelLeftClose as PanelLeftCloseIcon, 
  PanelLeftOpen as PanelLeftOpenIcon,
  Pause as PauseIcon,
  Play as PlayIcon,
  Download as DownloadIcon,
  RotateCcw as RotateCcwIcon,
  Trash as TrashIcon,
  Clock as ClockIcon,
  Folder as FolderIcon
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

// 获取状态徽章
function getStatusBadge(status: string) {
  const statusMap: Record<string, { label: string; variant: 'default' | 'secondary' | 'destructive' | 'outline' }> = {
    pending: { label: '等待中', variant: 'secondary' },
    chunking: { label: '分片中', variant: 'secondary' },
    queued: { label: '排队中', variant: 'secondary' },
    processing: { label: '处理中', variant: 'default' },
    completed: { label: '已完成', variant: 'outline' },
    failed: { label: '失败', variant: 'destructive' },
    paused: { label: '已暂停', variant: 'secondary' }
  }
  return statusMap[status] || { label: status, variant: 'secondary' as const }
}

// 格式化相对时间
function formatRelativeTime(dateStr: string) {
  const date = new Date(dateStr)
  const now = new Date()
  const diff = now.getTime() - date.getTime()
  const minutes = Math.floor(diff / 60000)
  const hours = Math.floor(diff / 3600000)
  const days = Math.floor(diff / 86400000)
  
  if (minutes < 1) return '刚刚'
  if (minutes < 60) return `${minutes}分钟前`
  if (hours < 24) return `${hours}小时前`
  if (days < 30) return `${days}天前`
  return date.toLocaleDateString('zh-CN')
}

// 键盘事件处理 - ESC 键关闭侧边栏
function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    if (showTaskSidebar.value) {
      showTaskSidebar.value = false
    } else if (!sidebarCollapsed.value) {
      sidebarCollapsed.value = true
    }
  }
}

// 监听侧边栏状态变化，管理焦点
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

// Get current task text for audio player
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

async function handleDownloadGroup(groupId: string) {
  try {
    await batchStore.downloadGroupAudio(groupId)
    toast.success('下载已开始')
  } catch (error) {
    toast.error('下载失败')
  }
}

function handleBatchImported(groupId: string) {
  selectedGroupId.value = groupId
  showBatchWizard.value = false
  batchStore.loadGroups()
  // Also refresh task list so newly created tasks appear
  taskStore.loadTasks()
}

// Lifecycle
onMounted(async () => {
  document.addEventListener('keydown', handleKeydown)
  await Promise.all([
    taskStore.loadTasks(),
    batchStore.loadGroups(),
  ])
})

onUnmounted(() => {
  document.removeEventListener('keydown', handleKeydown)
})
</script>

<style>
.slide-in-right-enter-active,
.slide-in-right-leave-active {
  transition: transform 0.3s ease;
}
.slide-in-right-enter-from,
.slide-in-right-leave-to {
  transform: translateX(100%);
}

.slide-in-left-enter-active,
.slide-in-left-leave-active {
  transition: transform 0.3s ease;
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
