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
          @input="onSearchInput"
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
    <div v-if="taskStore.loading && taskStore.standaloneTasks.length === 0" class="space-y-2 shrink-0">
      <Skeleton class="h-20 w-full" />
      <Skeleton class="h-20 w-full" />
      <Skeleton class="h-20 w-full" />
    </div>

    <!-- Scroll Container -->
    <div
      ref="scrollContainerRef"
      class="flex-1 overflow-y-auto scrollbar-auto min-h-0"
      v-else-if="taskStore.standaloneTasks.length > 0 || taskStore.totalCount > 0"
    >
      <!-- Virtual Scroller -->
      <div v-if="virtualRows.length > 0" class="pb-3">
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
            <template v-if="getRowType(virtualRow.index) === 'section-header'">
              <!-- Section Header -->
              <div
                class="flex items-center gap-2 py-2 cursor-pointer select-none group"
                @click="toggleCollapsed(getRowSection(virtualRow.index))"
              >
                <ChevronRightIcon
                  class="w-4 h-4 transition-transform duration-200 shrink-0"
                  :class="{ 'rotate-90': openSections.has(getRowSection(virtualRow.index)) }"
                />
                <template v-if="getRowSection(virtualRow.index) === 'pending'">
                  <Loader2Icon
                    v-if="openSections.has('pending')"
                    class="w-4 h-4 animate-spin text-primary shrink-0"
                  />
                  <span class="text-sm font-semibold text-primary">
                    进行中 ({{ pendingCount }})
                  </span>
                </template>
                <span
                  v-else-if="getRowSection(virtualRow.index) === 'completed'"
                  class="text-sm font-semibold text-muted-foreground group-hover:text-foreground transition-colors"
                >
                  已完成 ({{ completedCount }})
                </span>
                <span
                  v-else
                  class="text-sm font-semibold text-destructive group-hover:text-destructive/80 transition-colors"
                >
                  失败 ({{ failedCount }})
                </span>
              </div>
            </template>

            <!-- Task Item -->
            <template v-else-if="getRowType(virtualRow.index) === 'task'">
              <TaskItem
                :task="(getRowTask(virtualRow.index) as any)"
                :mode="getRowMode(virtualRow.index)"
                @play="handleOpenPlayer"
                @reuse="handleReuseConfig"
                @edit-title="handleEditTitle"
                @delete="handleDelete"
                @view-text="handleViewText"
              />
            </template>

            <!-- Loading skeleton -->
            <Skeleton
              v-else-if="getRowType(virtualRow.index) === 'skeleton'"
              class="h-20 w-full my-1"
            />
          </div>
        </div>
      </div>

      <!-- Empty state -->
      <div
        v-if="virtualRows.length === 0 && taskStore.standaloneTasks.length > 0"
        class="py-8 text-center text-muted-foreground"
      >
        <p class="text-sm">所有任务已完成</p>
      </div>

      <!-- Load more button -->
      <div v-if="taskStore.hasMore" class="pt-2">
        <Button 
          variant="outline" 
          size="sm" 
          class="w-full"
          @click="taskStore.loadMore" 
          :disabled="taskStore.loading"
        >
          <Loader2Icon v-if="taskStore.loading" class="w-4 h-4 animate-spin mr-2" />
          加载更多
        </Button>
      </div>
    </div>

    <!-- Empty state (no tasks at all) -->
    <div
      v-else
      class="flex-1 flex items-center justify-center text-muted-foreground text-sm"
    >
      暂无任务
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
import { ref, computed, watch, unref } from 'vue'
import { useVirtualizer } from '@tanstack/vue-virtual'
import { toast } from 'vue-sonner'
import { useTaskStore } from '@/stores/task'
import { api, type Task, type TaskSummary } from '@/api/client'
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
  | { type: 'task'; task: TaskSummary; mode: 'active' | 'completed' | 'failed' }
  | { type: 'skeleton' }

// ─── Store ──────────────────────────────────────────
const taskStore = useTaskStore()
const searchQuery = ref('')
const deleteTargetId = ref<string | null>(null)
const openSections = ref<Set<string>>(new Set(['pending']))
let searchTimer: ReturnType<typeof setTimeout> | null = null

// ─── Computed: Standalone (non-group) tasks ──────────
const filteredStandaloneTasks = computed(() => {
  const tasks = taskStore.standaloneTasks
  if (!searchQuery.value) return tasks
  const query = searchQuery.value.toLowerCase()
  return tasks.filter(t => 
    t.custom_title?.toLowerCase().includes(query) ||
    t.id.toLowerCase().includes(query)
  )
})

// ─── Group tasks by status ──────────────────────────
const pendingTasks = computed(() =>
  filteredStandaloneTasks.value.filter(t =>
    ['pending', 'queued', 'synthesizing', 'streaming'].includes(t.status)
  )
)

const completedTasks = computed(() =>
  filteredStandaloneTasks.value.filter(t => t.status === 'completed')
)

const failedTasks = computed(() =>
  filteredStandaloneTasks.value.filter(t => t.status === 'failed')
)

// ─── Virtual Rows ──────────────────────────────────
// ─── Counts for section headers ────────────────────
const pendingCount = computed(() =>
  filteredStandaloneTasks.value.filter(t =>
    ['pending', 'queued', 'synthesizing', 'streaming'].includes(t.status)
  ).length
)

const completedCount = computed(() =>
  filteredStandaloneTasks.value.filter(t => t.status === 'completed').length
)

const failedCount = computed(() =>
  filteredStandaloneTasks.value.filter(t => t.status === 'failed').length
)

// ─── Type-safe row accessors ──────────────────────
function getRowType(index: number): VirtualRow['type'] | undefined {
  return virtualRows.value[index]?.type
}

function getRowSection(index: number): 'pending' | 'completed' | 'failed' {
  const row = virtualRows.value[index]
  if (row?.type === 'section-header') return row.section
  return 'pending'
}

function getRowTask(index: number): TaskSummary | undefined {
  const row = virtualRows.value[index]
  if (row?.type === 'task') return row.task
  return undefined
}

function getRowMode(index: number): 'active' | 'completed' | 'failed' {
  const row = virtualRows.value[index]
  if (row?.type === 'task') return row.mode
  return 'active'
}

const virtualRows = computed<VirtualRow[]>(() => {
  const rows: VirtualRow[] = []

  if (pendingTasks.value.length > 0) {
    rows.push({ type: 'section-header', section: 'pending' })
    if (openSections.value.has('pending')) {
      for (const task of pendingTasks.value) {
        rows.push({ type: 'task', task, mode: 'active' })
      }
    }
  }

  if (completedTasks.value.length > 0) {
    rows.push({ type: 'section-header', section: 'completed' })
    if (openSections.value.has('completed')) {
      for (const task of completedTasks.value) {
        rows.push({ type: 'task', task, mode: 'completed' })
      }
    }
  }

  if (failedTasks.value.length > 0) {
    rows.push({ type: 'section-header', section: 'failed' })
    if (openSections.value.has('failed')) {
      for (const task of failedTasks.value) {
        rows.push({ type: 'task', task, mode: 'failed' })
      }
    }
  }

  return rows
})

// ─── Virtualizer ───────────────────────────────────
const scrollContainerRef = ref<HTMLElement | null>(null)

const virtualizer = useVirtualizer({
  get count() { return virtualRows.value.length },
  getScrollElement: () => scrollContainerRef.value as Element | null,
  estimateSize: (index: number) => {
    const item = virtualRows.value[index]
    if (!item) return 200
    if (item.type === 'section-header') return 40
    if (item.type === 'skeleton') return 84
    return 200
  },
  measureElement: (el: Element) => Math.max(el.getBoundingClientRect().height, 40),
  overscan: 5,
})

// ─── Infinite scroll: load more when near bottom ──
watch(
  () => unref(virtualizer).getVirtualItems(),
  (items) => {
    if (items.length === 0) return
    const lastItem = items[items.length - 1]
    if (lastItem && lastItem.index >= virtualRows.value.length - 3) {
      if (taskStore.hasMore && !taskStore.loading) {
        taskStore.loadMore()
      }
    }
  },
  { deep: true }
)

// ─── Collapse Toggle ──────────────────────────────
function toggleCollapsed(section: 'pending' | 'completed' | 'failed') {
  const newSet = new Set(openSections.value)
  if (newSet.has(section)) {
    newSet.delete(section)
  } else {
    newSet.add(section)
  }
  openSections.value = newSet
}

// ─── Search ────────────────────────────────────────
function onSearchInput() {
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(() => {
    taskStore.searchTasks(searchQuery.value)
  }, 300)
}

// ─── Emits ─────────────────────────────────────────
const emit = defineEmits<{
  'open-player': [task: Task]
  'reuse-config': [config: { text: string; voice: string | null; model: string; context?: string }]
  'open-text-viewer': [task: Task]
}>()

// ─── Handlers ─────────────────────────────────────
async function handleOpenPlayer(task: Task | TaskSummary) {
  const t = task as Task
  if ('text' in t && typeof t.text === 'string') {
    emit('open-player', t)
  } else {
    try {
      const full = await taskStore.getTaskDetail(task.id)
      emit('open-player', full)
    } catch (error) {
      handleApiError(error, '加载任务详情失败')
    }
  }
}

async function handleReuseConfig(task: Task | TaskSummary) {
  try {
    const full = 'text' in (task as Task) && typeof (task as Task).text === 'string'
      ? (task as Task)
      : await taskStore.getTaskDetail(task.id)
    emit('reuse-config', {
      text: full.text,
      voice: full.voice,
      model: full.model,
      context: full.context || '',
    })
  } catch (error) {
    handleApiError(error, '加载任务详情失败')
  }
}

async function handleViewText(task: Task | TaskSummary) {
  const t = task as Task
  if ('text' in t && typeof t.text === 'string') {
    emit('open-text-viewer', t)
  } else {
    try {
      const full = await taskStore.getTaskDetail(task.id)
      emit('open-text-viewer', full)
    } catch (error) {
      handleApiError(error, '加载任务详情失败')
    }
  }
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
