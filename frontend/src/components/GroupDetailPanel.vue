<template>
  <div class="w-full h-full">
    <div class="flex flex-col h-full border rounded-lg bg-card shadow-sm">
      <!-- Header -->
      <div class="p-4 border-b shrink-0">
        <div class="flex items-center justify-between">
          <div class="min-w-0 flex-1">
            <h3 class="text-lg font-semibold truncate">{{ group.name }}</h3>
            <div class="flex items-center gap-3 mt-1">
              <Badge :variant="statusVariant">{{ statusLabel }}</Badge>
              <span class="text-sm text-muted-foreground">
                {{ group.completed_tasks }}/{{ group.total_tasks }} 完成
              </span>
              <span v-if="group.failed_tasks > 0" class="text-sm text-destructive">
                {{ group.failed_tasks }} 失败
              </span>
            </div>
          </div>
          <div class="flex items-center gap-2">
            <Button
              v-if="group.status === 'processing'"
              variant="outline"
              size="sm"
              @click="$emit('pause', group.id)"
            >
              <PauseIcon class="w-4 h-4 mr-1" />
              暂停
            </Button>
            <Button
              v-if="group.status === 'paused'"
              variant="outline"
              size="sm"
              @click="$emit('resume', group.id)"
            >
              <PlayIcon class="w-4 h-4 mr-1" />
              恢复
            </Button>
            <Button
              v-if="group.failed_tasks > 0"
              variant="outline"
              size="sm"
              @click="$emit('retry', group.id)"
            >
              <RotateCcwIcon class="w-4 h-4 mr-1" />
              重试
            </Button>
            <Button
              v-if="group.completed_tasks > 0"
              variant="outline"
              size="sm"
              :disabled="downloading"
              @click="$emit('download', group.id)"
            >
              <DownloadIcon class="w-4 h-4 mr-1" />
              {{ downloading ? '打包中...' : '下载全部' }}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              class="h-8 w-8 p-0"
              :disabled="refreshing"
              @click="refreshTasks"
              title="刷新"
            >
              <RefreshCwIcon class="w-4 h-4" :class="refreshing && 'animate-spin'" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              class="h-8 w-8 p-0"
              @click="$emit('close')"
            >
              <XIcon class="w-4 h-4" />
            </Button>
          </div>
        </div>

        <!-- Progress -->
        <div v-if="group.status === 'processing' || group.status === 'paused'" class="mt-3">
          <Progress :model-value="progress" class="h-2" />
        </div>
      </div>

      <!-- Kanban Board -->
      <div class="flex-1 min-h-0 overflow-x-auto p-4">
        <!-- Loading -->
        <div v-if="loading" class="flex gap-4 h-full">
          <div v-for="i in 4" :key="i" class="flex-1 min-w-[250px]">
            <Skeleton class="h-full w-full" />
          </div>
        </div>

        <!-- Empty -->
        <div v-else-if="groupTasks.length === 0" class="flex items-center justify-center h-full text-muted-foreground">
          暂无任务
        </div>

        <!-- Kanban Columns -->
        <div v-else class="flex gap-4 min-w-[1000px]" style="height: calc(100vh - 280px); max-height: 700px;">
          <div
            v-for="(column, columnIndex) in kanbanColumns"
            :key="column.id"
            class="flex-1 min-w-[250px] flex flex-col min-h-0"
          >
            <!-- Column Header -->
            <div class="flex items-center gap-2 mb-3 pb-2 border-b shrink-0">
              <div :class="['w-2.5 h-2.5 rounded-full', column.dotClass]" />
              <h4 class="text-sm font-medium">{{ column.title }}</h4>
              <Badge variant="secondary" class="ml-auto text-xs">
                {{ column.tasks.length }}
              </Badge>
            </div>

            <!-- Column Content (virtual scroller) -->
            <div
              :ref="(el: any) => { if (el) columnScrollRefs[columnIndex] = el as HTMLElement }"
              class="flex-1 min-h-0 overflow-y-auto scrollbar-auto pr-1"
            >
              <!-- Virtual scroller when tasks exist -->
              <template v-if="column.tasks.length > 0">
                <div
                  :style="{ height: `${columnVirtualizers[columnIndex]?.value?.getTotalSize() ?? 0}px` }"
                  class="relative w-full"
                >
                  <div
                    v-for="virtualRow in columnVirtualizers[columnIndex]?.value?.getVirtualItems() ?? []"
                    :key="`col-${columnIndex}-${virtualRow.index}`"
                    :data-index="virtualRow.index"
                    :ref="(el: any) => { if (el?.nodeType === 1) columnVirtualizers[columnIndex]?.value?.measureElement(el) }"
                    class="absolute left-0 w-full"
                    :style="{ transform: `translateY(${virtualRow.start}px)` }"
                  >
                    <div class="mx-0 mb-2 p-3 rounded-lg border bg-card hover:bg-muted/50 transition-colors">
                      <div class="flex items-start justify-between gap-2">
                        <div class="min-w-0 flex-1">
                          <p class="text-sm font-medium truncate">
                            {{ columnTask(column, virtualRow.index).custom_title || columnTask(column, virtualRow.index).title || columnTask(column, virtualRow.index).id.slice(0, 8) }}
                          </p>
                          <div class="flex items-center gap-2 mt-1.5">
                            <Badge :variant="taskStatusVariant(columnTask(column, virtualRow.index).status)" class="text-xs">
                              {{ taskStatusLabel(columnTask(column, virtualRow.index).status) }}
                            </Badge>
                          </div>
                        </div>
                        <div class="flex items-center gap-1 shrink-0">
                          <Button
                            v-if="columnTask(column, virtualRow.index).status === 'done' && columnTask(column, virtualRow.index).has_audio"
                            variant="ghost"
                            size="sm"
                            class="h-7 w-7 p-0"
                            title="播放"
                            @click="$emit('play', columnTask(column, virtualRow.index))"
                          >
                            <PlayIcon class="w-3.5 h-3.5" />
                          </Button>
                          <Button
                            v-if="canCancelTask(columnTask(column, virtualRow.index).status)"
                            variant="ghost"
                            size="sm"
                            class="h-7 w-7 p-0 text-destructive hover:text-destructive"
                            title="取消"
                            @click="handleCancelTask(columnTask(column, virtualRow.index).id)"
                          >
                            <XCircleIcon class="w-3.5 h-3.5" />
                          </Button>
                          <Button
                            variant="ghost"
                            size="sm"
                            class="h-7 w-7 p-0"
                            title="查看原文"
                            @click="$emit('view-text', columnTask(column, virtualRow.index))"
                          >
                            <FileTextIcon class="w-3.5 h-3.5" />
                          </Button>
                        </div>
                      </div>

                      <!-- Progress bar for active tasks -->
                      <div v-if="isActiveStatus(columnTask(column, virtualRow.index).status)" class="mt-2">
                        <div class="flex items-center gap-2">
                          <div class="flex-1 h-1.5 bg-muted rounded-full overflow-hidden">
                            <div
                              class="h-full bg-primary rounded-full transition-all duration-500 animate-pulse"
                              :style="{ width: `${statusProgress(columnTask(column, virtualRow.index).status)}%` }"
                            />
                          </div>
                          <span class="text-xs text-muted-foreground">
                            {{ statusProgress(columnTask(column, virtualRow.index).status) }}%
                          </span>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </template>

              <!-- Empty column message -->
              <template v-else>
                <div class="text-center py-8 text-muted-foreground text-sm">
                  {{ column.emptyText }}
                </div>
              </template>
            </div>
          </div>
        </div>
      </div>

      <!-- Stats Footer -->
      <div class="p-3 border-t text-xs text-muted-foreground shrink-0">
        <div class="flex justify-between">
          <span>总 Tokens: {{ formatTokens(group.total_tokens) }}</span>
          <span>创建于: {{ formatDate(group.created_at) }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { useVirtualizer } from '@tanstack/vue-virtual'
import type { GroupSummary, TaskSummary } from '@/api/client'
import { apiV2 } from '@/api/client'
import { useBatchStore } from '@/stores/batch'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { Skeleton } from '@/components/ui/skeleton'
import {
  X as XIcon,
  Pause as PauseIcon,
  Play as PlayIcon,
  RotateCcw as RotateCcwIcon,
  Download as DownloadIcon,
  FileText as FileTextIcon,
  RefreshCw as RefreshCwIcon,
  XCircle as XCircleIcon,
} from 'lucide-vue-next'
import type { BadgeVariants } from '@/components/ui/badge'

const props = defineProps<{
  group: GroupSummary
  downloading?: boolean
}>()

defineEmits<{
  close: []
  pause: [groupId: string]
  resume: [groupId: string]
  retry: [groupId: string]
  download: [groupId: string]
  play: [task: TaskSummary]
  'view-text': [task: TaskSummary]
  'cancel-task': [taskId: string]
}>()

const batchStore = useBatchStore()
const loading = ref(false)
const refreshing = ref(false)
let pollingTimer: ReturnType<typeof setInterval> | null = null

// ─── Refresh tasks ────────────────────────────────────
async function refreshTasks() {
  if (refreshing.value) return
  refreshing.value = true
  try {
    await batchStore.getGroupDetailWithTasks(props.group.id, 0, 100)
  } catch (error) {
    console.error('Failed to refresh group tasks:', error)
  } finally {
    refreshing.value = false
  }
}

// ─── Auto-polling for active groups ───────────────────
function startPolling() {
  stopPolling()
  pollingTimer = setInterval(() => {
    if (isActiveStatus(props.group.status)) {
      refreshTasks()
    }
  }, 5000) // Poll every 5s
}

function stopPolling() {
  if (pollingTimer) {
    clearInterval(pollingTimer)
    pollingTimer = null
  }
}

// Watch group status to start/stop polling
watch(() => props.group.status, (status) => {
  if (isActiveStatus(status)) {
    startPolling()
  } else {
    stopPolling()
  }
}, { immediate: true })

// ─── Group tasks from store cache ─────────────────────
const groupTasks = computed<TaskSummary[]>(() => {
  return batchStore.getGroupTasks(props.group.id)
})

// ─── Kanban columns ─────────────────────────────────
interface KanbanColumn {
  id: string
  title: string
  dotClass: string
  emptyText: string
  tasks: TaskSummary[]
  statuses: string[]
}

const kanbanColumns = computed<KanbanColumn[]>(() => {
  const tasks = groupTasks.value
  
  return [
    {
      id: 'queued',
      title: '等待中',
      dotClass: 'bg-yellow-500',
      emptyText: '无等待任务',
      statuses: ['pending', 'queued'],
      tasks: tasks.filter(t => ['pending', 'queued'].includes(t.status)),
    },
    {
      id: 'processing',
      title: '处理中',
      dotClass: 'bg-blue-500 animate-pulse',
      emptyText: '无处理中任务',
      statuses: ['chunking', 'processing', 'merging'],
      tasks: tasks.filter(t => ['chunking', 'processing', 'merging'].includes(t.status)),
    },
    {
      id: 'done',
      title: '已完成',
      dotClass: 'bg-green-500',
      emptyText: '无已完成任务',
      statuses: ['completed', 'done'],
      tasks: tasks.filter(t => ['completed', 'done'].includes(t.status)),
    },
    {
      id: 'failed',
      title: '失败',
      dotClass: 'bg-red-500',
      emptyText: '无失败任务',
      statuses: ['failed', 'merging_failed', 'cancelled'],
      tasks: tasks.filter(t => ['failed', 'merging_failed', 'cancelled'].includes(t.status)),
    },
  ]
})

// ─── Virtual scrolling for columns ────────────────────
const columnScrollRefs = ref<HTMLElement[]>([null!, null!, null!, null!])

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const columnVirtualizers: any[] = []
for (let i = 0; i < 4; i++) {
  const idx = i
  columnVirtualizers.push(
    useVirtualizer({
      get count() {
        return kanbanColumns.value[idx]?.tasks.length ?? 0
      },
      getScrollElement: () => columnScrollRefs.value[idx],
      estimateSize: () => 120,
      overscan: 5,
    }),
  )
}

function columnTask(column: KanbanColumn, index: number): TaskSummary {
  return column.tasks[index]
}

// ─── UI helpers ──────────────────────────────────────

const statusVariant = computed(() => {
  switch (props.group.status) {
    case 'pending': return 'secondary' as const
    case 'queued': return 'secondary' as const
    case 'processing': return 'default' as const
    case 'paused': return 'outline' as const
    case 'completed': return 'success' as const
    case 'failed': return 'destructive' as const
    default: return 'secondary' as const
  }
})

const statusLabel = computed(() => {
  switch (props.group.status) {
    case 'pending': return '等待中'
    case 'queued': return '队列中'
    case 'processing': return '处理中'
    case 'paused': return '已暂停'
    case 'completed': return '已完成'
    case 'failed': return '失败'
    default: return props.group.status
  }
})

const progress = computed(() => {
  if (props.group.total_tasks === 0) return 0
  return (props.group.completed_tasks / props.group.total_tasks) * 100
})

function taskStatusVariant(status: string): BadgeVariants['variant'] {
  switch (status) {
    case 'completed':
    case 'done': return 'success'
    case 'failed':
    case 'merging_failed': return 'destructive'
    case 'processing':
    case 'chunking':
    case 'merging': return 'default'
    case 'queued':
    case 'pending': return 'warning'
    case 'paused': return 'outline'
    case 'cancelled': return 'secondary'
    default: return 'secondary'
  }
}

function taskStatusLabel(status: string): string {
  switch (status) {
    case 'pending': return '等待'
    case 'queued': return '队列中'
    case 'chunking': return '分片中'
    case 'processing': return '合成中'
    case 'merging': return '合并中'
    case 'merging_failed': return '合并失败'
    case 'completed':
    case 'done': return '完成'
    case 'failed': return '失败'
    case 'paused': return '暂停'
    case 'cancelled': return '取消'
    default: return status
  }
}

function isActiveStatus(status: string): boolean {
  return ['queued', 'chunking', 'processing', 'merging'].includes(status)
}

function statusProgress(status: string): number {
  switch (status) {
    case 'queued': return 10
    case 'chunking': return 30
    case 'processing': return 60
    case 'merging': return 90
    default: return 0
  }
}

function formatTokens(tokens: number): string {
  if (!tokens) return '0'
  if (tokens >= 1000000) return (tokens / 1000000).toFixed(1) + 'M'
  if (tokens >= 1000) return (tokens / 1000).toFixed(1) + 'K'
  return tokens.toString()
}

function formatDate(dateStr: string): string {
  if (!dateStr) return '-'
  const date = new Date(dateStr)
  return date.toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

// ─── Cancel task ────────────────────────────────────

function canCancelTask(status: string): boolean {
  return ['pending', 'queued', 'chunking', 'processing', 'merging', 'mergingfailed', 'paused'].includes(status)
}

async function handleCancelTask(taskId: string) {
  try {
    await apiV2.cancelTask(taskId)
    // Refresh to reflect the cancelled state
    await refreshTasks()
  } catch (error) {
    console.error('Failed to cancel task:', error)
  }
}

// ─── Load tasks on mount ─────────────────────────────

onMounted(async () => {
  loading.value = true
  try {
    await batchStore.getGroupDetailWithTasks(props.group.id, 0, 100)
  } catch (error) {
    console.error('Failed to load group tasks:', error)
  } finally {
    loading.value = false
  }
  // Start polling if group is active
  if (isActiveStatus(props.group.status)) {
    startPolling()
  }
})

onUnmounted(() => {
  stopPolling()
})
</script>
