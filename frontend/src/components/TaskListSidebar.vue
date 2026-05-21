<template>
  <div class="flex flex-col h-full p-3 sm:p-4">
    <!-- Search + Refresh Row -->
    <div class="flex items-center gap-2 shrink-0 pb-3">
      <div class="relative flex-1">
        <SearchIcon class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
        <Input
          v-model="searchQuery"
          placeholder="搜索任务名称或ID..."
          class="pl-9 text-sm"
        />
      </div>
      <Button 
        variant="outline" 
        size="sm"
        class="shrink-0 px-2.5"
        @click="refreshTasks" 
        :disabled="taskStore.refreshing"
      >
        <Loader2Icon v-if="taskStore.refreshing" class="w-4 h-4 animate-spin" />
        <span v-else>刷新</span>
      </Button>
    </div>

    <!-- Loading State (仅首次加载) -->
    <div v-if="taskStore.loading && taskStore.tasks.length === 0" class="space-y-2 shrink-0">
      <Skeleton class="h-20 w-full" />
      <Skeleton class="h-20 w-full" />
      <Skeleton class="h-20 w-full" />
    </div>

    <!-- Scroll Container: all sections virtualized (pending + completed + failed) -->
    <div
      ref="scrollContainerRef"
      class="flex-1 overflow-y-auto min-h-0"
      v-if="taskStore.tasks.length > 0 && !(taskStore.loading && taskStore.tasks.length === 0)"
    >
      <!-- Virtual Scroller for All Sections -->
      <div v-if="flatItems.length > 0" class="pb-3">
        <div :style="{ height: `${virtualizer.getTotalSize()}px` }" class="relative w-full">
          <div
            v-for="virtualRow in virtualizer.getVirtualItems()"
            :key="`v-${virtualRow.index}`"
            :data-index="virtualRow.index"
            :ref="(el: any) => { if (el?.nodeType === 1) virtualizer.measureElement(el) }"
            class="absolute left-0 w-full"
            :style="{
              transform: `translateY(${virtualRow.start}px)`,
            }"
          >
            <!-- Section Header -->
            <div
              v-if="flatItems[virtualRow.index]?.type === 'section-header'"
              class="flex items-center gap-2 py-2 cursor-pointer select-none group"
              @click="toggleCollapsed(flatItems[virtualRow.index].section)"
            >
              <ChevronRightIcon
                class="w-4 h-4 transition-transform duration-200 shrink-0"
                :class="{ 'rotate-90': expandedSection === flatItems[virtualRow.index].section }"
              />
              <template v-if="flatItems[virtualRow.index].section === 'pending'">
                <Loader2Icon
                  v-if="expandedSection === 'pending'"
                  class="w-4 h-4 animate-spin text-primary shrink-0"
                />
                <span class="text-sm font-semibold text-primary">
                  进行中 ({{ filteredPendingTasks.length }})
                </span>
              </template>
              <span
                v-else-if="flatItems[virtualRow.index].section === 'completed'"
                class="text-sm font-semibold text-muted-foreground group-hover:text-foreground transition-colors"
              >
                已完成 ({{ filteredCompletedTasks.length }})
              </span>
              <span
                v-else
                class="text-sm font-semibold text-destructive group-hover:text-destructive/80 transition-colors"
              >
                失败 ({{ filteredFailedTasks.length }})
              </span>
            </div>

            <!-- Task Item -->
            <template v-else-if="flatItems[virtualRow.index]?.type === 'task'">
              <TaskItem
                :task="flatItems[virtualRow.index].task"
                :mode="flatItems[virtualRow.index].mode"
                @play="handleOpenPlayer"
                @reuse="handleReuseConfig"
                @edit-title="handleEditTitle"
                @delete="handleDelete"
                @view-text="handleViewText"
              />
            </template>
          </div>
        </div>
      </div>

      <!-- Empty virtual area message -->
      <div
        v-if="flatItems.length === 0 && taskStore.tasks.length > 0"
        class="py-8 text-center text-muted-foreground"
      >
        <p class="text-sm">所有任务已完成</p>
      </div>

      <!-- Bottom refresh (inside scroll container so it scrolls naturally) -->
      <div class="pt-2">
        <Button 
          variant="outline" 
          size="sm" 
          class="w-full"
          @click="taskStore.loadTasks" 
          :disabled="taskStore.refreshing"
        >
          <Loader2Icon v-if="taskStore.refreshing" class="w-4 h-4 animate-spin mr-2" />
          刷新列表
        </Button>
      </div>
    </div>

    <!-- Delete Confirmation Dialog -->
    <Dialog :open="deleteTargetId !== null" @update:open="(open) => { if (!open) deleteTargetId = null }">
      <DialogContent class="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>确认删除</DialogTitle>
          <DialogDescription>
            确定要删除此任务吗？此操作不可撤销。
          </DialogDescription>
        </DialogHeader>
        <DialogFooter class="flex sm:flex-row gap-2">
          <Button variant="outline" @click="deleteTargetId = null">
            取消
          </Button>
          <Button variant="destructive" @click="confirmDelete">
            确认删除
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useVirtualizer } from '@tanstack/vue-virtual'
import { toast } from 'vue-sonner'
import { useTaskStore } from '@/stores/task'
import { api, type Task } from '@/api/client'
import { handleApiError } from '@/utils/errorHandler'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog'
import TaskItem from './TaskItem.vue'
import { 
  Loader2 as Loader2Icon,
  ChevronRight as ChevronRightIcon,
  Search as SearchIcon
} from 'lucide-vue-next'

// ─── Types ──────────────────────────────────────────
type VirtualRow =
  | { type: 'section-header'; section: 'pending' | 'completed' | 'failed' }
  | { type: 'task'; task: Task; mode: 'active' | 'completed' | 'failed' }

// ─── Store ──────────────────────────────────────────
const taskStore = useTaskStore()
const searchQuery = ref('')
const deleteTargetId = ref<string | null>(null)
const expandedSection = ref<string | null>('pending')

// ─── Computed: Filtered Tasks ──────────────────────
const pendingTasks = computed(() => 
  taskStore.tasks.filter(t => 
    ['pending', 'queued', 'synthesizing', 'streaming'].includes(t.status)
  )
)

const completedTasks = computed(() => 
  taskStore.tasks.filter(t => t.status === 'completed')
)

const failedTasks = computed(() => 
  taskStore.tasks.filter(t => t.status === 'failed')
)

const filteredPendingTasks = computed(() => {
  if (!searchQuery.value) return pendingTasks.value
  const query = searchQuery.value.toLowerCase()
  return pendingTasks.value.filter(t => 
    t.custom_title?.toLowerCase().includes(query) ||
    t.id.toLowerCase().includes(query)
  )
})

const filteredCompletedTasks = computed(() => {
  if (!searchQuery.value) return completedTasks.value
  const query = searchQuery.value.toLowerCase()
  return completedTasks.value.filter(t => 
    t.custom_title?.toLowerCase().includes(query) ||
    t.id.toLowerCase().includes(query)
  )
})

const filteredFailedTasks = computed(() => {
  if (!searchQuery.value) return failedTasks.value
  const query = searchQuery.value.toLowerCase()
  return failedTasks.value.filter(t => 
    t.custom_title?.toLowerCase().includes(query) ||
    t.id.toLowerCase().includes(query)
  )
})

// ─── Flattened Virtual Items ───────────────────────
const virtualItems = computed<VirtualRow[]>(() => {
  const items: VirtualRow[] = []

  if (filteredPendingTasks.value.length > 0) {
    items.push({ type: 'section-header', section: 'pending' })
    if (expandedSection.value === 'pending') {
      filteredPendingTasks.value.forEach(task => {
        items.push({ type: 'task', task, mode: 'active' })
      })
    }
  }

  if (filteredCompletedTasks.value.length > 0) {
    items.push({ type: 'section-header', section: 'completed' })
    if (expandedSection.value === 'completed') {
      filteredCompletedTasks.value.forEach(task => {
        items.push({ type: 'task', task, mode: 'completed' })
      })
    }
  }

  if (filteredFailedTasks.value.length > 0) {
    items.push({ type: 'section-header', section: 'failed' })
    if (expandedSection.value === 'failed') {
      filteredFailedTasks.value.forEach(task => {
        items.push({ type: 'task', task, mode: 'failed' })
      })
    }
  }

  return items
})

// Flattened as any[] for template use (avoids discriminated union narrowing issues)
const flatItems = computed(() => virtualItems.value as any[])

// ─── Virtualizer ───────────────────────────────────
const scrollContainerRef = ref<HTMLElement | null>(null)

const virtualizer = useVirtualizer({
  get count() { return virtualItems.value.length },
  getScrollElement: () => scrollContainerRef.value as Element | null,
  estimateSize: (index: number) => {
    const item = virtualItems.value[index]
    return item?.type === 'section-header' ? 40 : 200
  },
  measureElement: (el: Element) => Math.max(el.getBoundingClientRect().height, 40),
  overscan: 5,
})

// ─── Collapse Toggle ──────────────────────────────
function toggleCollapsed(section: 'pending' | 'completed' | 'failed') {
  expandedSection.value = expandedSection.value === section ? null : section
}

// ─── Emits ─────────────────────────────────────────
const emit = defineEmits<{
  'open-player': [taskId: string]
  'reuse-config': [config: { text: string; voice: string | null; model: string; context?: string }]
  'open-text-viewer': [task: Task]
}>()

// ─── Handlers ─────────────────────────────────────
function handleOpenPlayer(taskId: string) {
  emit('open-player', taskId)
}

function handleReuseConfig(task: Task) {
  emit('reuse-config', {
    text: task.text,
    voice: task.voice,
    model: task.model,
    context: task.context || '',
  })
}

function handleViewText(task: Task) {
  emit('open-text-viewer', task)
}

async function handleEditTitle(taskId: string, newTitle: string) {
  try {
    await api.updateTaskTitle(taskId, newTitle)
    toast.success('标题已更新')
    await taskStore.loadTasks()
  } catch (error) {
    handleApiError(error, '更新标题失败')
  }
}

function handleDelete(taskId: string) {
  deleteTargetId.value = taskId
}

async function confirmDelete() {
  if (!deleteTargetId.value) return
  const taskId = deleteTargetId.value
  deleteTargetId.value = null
  try {
    await taskStore.removeTask(taskId)
    toast.success('任务已删除')
  } catch (error) {
    toast.error('删除失败')
  }
}

async function refreshTasks() {
  await taskStore.loadTasks()
  toast.success('列表已刷新')
}
</script>

<style>
.task-list-enter-active,
.task-list-leave-active {
  transition: all 0.3s ease;
}
.task-list-enter-from {
  opacity: 0;
  transform: translateY(-8px);
}
.task-list-leave-to {
  opacity: 0;
  transform: translateY(8px);
}
.task-list-move {
  transition: transform 0.3s ease;
}
</style>
