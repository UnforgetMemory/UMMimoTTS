<template>
  <div class="h-full overflow-y-auto">
    <!-- Loading skeleton -->
    <div v-if="initialLoading" class="flex flex-col h-full p-6 gap-6">
      <div class="flex items-center gap-3">
        <Skeleton class="h-8 w-8 rounded-md" />
        <Skeleton class="h-6 w-48" />
        <Skeleton class="h-5 w-16 rounded-full" />
      </div>
      <div class="flex gap-3">
        <Skeleton class="h-9 w-20 rounded-md" />
        <Skeleton class="h-9 w-20 rounded-md" />
        <Skeleton class="h-9 w-20 rounded-md" />
        <Skeleton class="h-9 w-20 rounded-md" />
      </div>
      <Skeleton class="h-3 w-full rounded-full" />
      <div class="flex-1 flex gap-4 min-h-0">
        <div v-for="col in 4" :key="col" class="flex-1 min-w-[240px] flex flex-col min-h-0 gap-3">
          <div class="flex items-center gap-2 pb-2 border-b">
            <Skeleton class="w-2 h-2 rounded-full" />
            <Skeleton class="h-4 w-16" />
            <Skeleton class="h-4 w-7 ml-auto rounded" />
          </div>
          <div class="space-y-2">
            <div v-for="card in 4" :key="card" class="p-3 rounded-lg border">
              <div class="space-y-2">
                <Skeleton class="h-4 w-4/5" />
                <Skeleton class="h-4 w-12 rounded-md" />
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Fetch error state -->
    <div
      v-else-if="fetchError"
      class="flex flex-col items-center justify-center h-full text-muted-foreground gap-4"
    >
      <div class="w-16 h-16 rounded-full bg-muted/50 flex items-center justify-center">
        <AlertCircleIcon class="w-8 h-8 text-muted-foreground/40" />
      </div>
      <div class="text-center">
        <p class="text-base font-medium text-foreground/80">加载失败</p>
        <p class="text-sm text-muted-foreground/60 mt-1">{{ fetchError }}</p>
      </div>
      <Button variant="outline" size="sm" class="mt-2" @click="handleRefresh">
        <RotateCcwIcon class="w-4 h-4 mr-1.5" />
        重试
      </Button>
    </div>

    <!-- 404 state -->
    <div
      v-else-if="!group"
      class="flex flex-col items-center justify-center h-full text-muted-foreground gap-4"
    >
      <div class="w-16 h-16 rounded-full bg-muted/50 flex items-center justify-center">
        <AlertCircleIcon class="w-8 h-8 text-muted-foreground/40" />
      </div>
      <div class="text-center">
        <p class="text-base font-medium text-foreground/80">分组未找到</p>
        <p class="text-sm text-muted-foreground/60 mt-1">该分组不存在或已被删除</p>
      </div>
      <Button variant="outline" size="sm" class="mt-2" @click="router.push('/tasks/batch')">
        <ArrowLeftIcon class="w-4 h-4 mr-1.5" />
        返回批量任务
      </Button>
    </div>

    <!-- Main content -->
    <div v-else class="flex flex-col h-full border rounded-xl bg-card shadow-sm overflow-hidden">
      <!-- Header -->
      <div class="p-4 sm:p-5 border-b shrink-0 bg-card">
        <div class="flex items-start justify-between gap-3">
          <div class="flex items-start gap-3 min-w-0 flex-1">
            <Button
              variant="ghost"
              size="sm"
              class="h-8 w-8 p-0 text-muted-foreground hover:text-foreground shrink-0 mt-0.5"
              title="返回"
              @click="router.push('/tasks/batch')"
            >
              <ArrowLeftIcon class="w-4 h-4" />
            </Button>
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2.5">
                <h1 class="text-base sm:text-lg font-semibold truncate text-foreground">
                  {{ group.name }}
                </h1>
                <Badge :variant="statusVariant" class="shrink-0 text-[10px] leading-tight px-1.5 py-0">
                  {{ statusLabel }}
                </Badge>
              </div>
              <div class="flex items-center gap-4 mt-1.5 text-xs text-muted-foreground">
                <span class="tabular-nums">
                  {{ group.completed_tasks }}/{{ group.total_tasks }}
                  <span class="text-muted-foreground/60">完成</span>
                </span>
                <span v-if="group.failed_tasks > 0" class="text-destructive tabular-nums font-medium">
                  {{ group.failed_tasks }}
                  <span class="text-destructive/70">失败</span>
                </span>
                <span class="tabular-nums text-muted-foreground/60">
                  {{ formatTokens(group.total_tokens) }}
                  <span class="text-muted-foreground/50">tokens</span>
                </span>
              </div>
            </div>
          </div>

          <!-- Action buttons -->
          <div class="flex items-center gap-1.5 shrink-0 flex-wrap">
            <Button
              v-if="group.status === 'processing'"
              variant="outline"
              size="sm"
              class="h-8 text-xs"
              @click="handlePause"
            >
              <PauseIcon class="w-3.5 h-3.5 mr-1" />
              暂停
            </Button>
            <Button
              v-if="group.status === 'paused'"
              variant="outline"
              size="sm"
              class="h-8 text-xs"
              @click="handleResume"
            >
              <PlayIcon class="w-3.5 h-3.5 mr-1" />
              恢复
            </Button>
            <Button
              v-if="group.failed_tasks > 0"
              variant="outline"
              size="sm"
              class="h-8 text-xs"
              @click="handleRetry"
            >
              <RotateCcwIcon class="w-3.5 h-3.5 mr-1" />
              重试
            </Button>
            <Button
              v-if="group.completed_tasks > 0"
              variant="outline"
              size="sm"
              class="h-8 text-xs"
              :disabled="downloading"
              @click="handleDownload"
            >
              <DownloadIcon class="w-3.5 h-3.5 mr-1" />
              {{ downloading ? '打包中...' : '下载全部' }}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              class="h-8 w-8 p-0 text-muted-foreground hover:text-foreground"
              :disabled="refreshing"
              title="刷新"
              @click="handleRefresh"
            >
              <RefreshCwIcon class="w-4 h-4" :class="refreshing && 'animate-spin'" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              class="h-8 w-8 p-0 text-muted-foreground hover:text-destructive"
              title="删除"
              @click="handleDelete"
            >
              <Trash2Icon class="w-4 h-4" />
            </Button>
          </div>
        </div>

        <!-- Progress bar -->
        <div v-if="progress > 0" class="mt-3">
          <div class="flex items-center gap-2">
            <div class="flex-1 h-2 bg-muted/60 rounded-full overflow-hidden">
              <div
                class="h-full rounded-full transition-all duration-500 ease-out"
                :class="group.status === 'paused' ? 'bg-muted-foreground/40' : 'bg-primary'"
                :style="{ width: `${progress}%` }"
              />
            </div>
            <span class="text-xs tabular-nums text-muted-foreground font-medium shrink-0">
              {{ Math.round(progress) }}%
            </span>
          </div>
        </div>
      </div>

      <!-- Kanban Board -->
      <div class="flex-1 min-h-0 overflow-x-auto p-4 sm:p-5">
        <!-- Loading tasks -->
        <div v-if="loadingTasks" class="flex gap-4 min-w-[900px] xl:min-w-[1100px] 2xl:min-w-[1300px] h-[calc(100vh-280px)] max-h-[700px]">
          <div v-for="col in 4" :key="col" class="flex-1 min-w-[240px] flex flex-col min-h-0">
            <div class="flex items-center gap-2 mb-3 pb-2 shrink-0">
              <Skeleton class="w-2 h-2 rounded-full" />
              <Skeleton class="h-4 w-16" />
              <Skeleton class="h-4 w-7 ml-auto rounded" />
            </div>
            <div class="flex-1 min-h-0 overflow-hidden space-y-2">
              <div v-for="card in 4" :key="card" class="p-3 rounded-lg border bg-card">
                <div class="space-y-2">
                  <Skeleton class="h-4 w-4/5" />
                  <Skeleton class="h-4 w-12 rounded-md" />
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- Empty state -->
        <div
          v-else-if="groupTasks.length === 0"
          class="flex flex-col items-center justify-center h-full text-muted-foreground gap-3"
        >
          <div class="w-14 h-14 rounded-full bg-muted/50 flex items-center justify-center">
            <ListIcon class="w-7 h-7 text-muted-foreground/40" />
          </div>
          <div class="text-center">
            <p class="text-sm font-medium">暂无任务数据</p>
            <p class="text-xs text-muted-foreground/60 mt-1">该分组尚未加载任务详情</p>
          </div>
        </div>

        <!-- Kanban columns -->
        <div
          v-else
          class="flex gap-4 min-w-[900px] xl:min-w-[1100px] 2xl:min-w-[1300px] h-[calc(100vh-280px)] max-h-[700px]"
        >
          <div
            v-for="(column, columnIndex) in kanbanColumns"
            :key="column.id"
            class="flex-1 min-w-[240px] flex flex-col min-h-0"
          >
            <!-- Column header -->
            <div class="flex items-center gap-2 mb-3 pb-2.5 border-b shrink-0">
              <div :class="['w-2 h-2 rounded-full', column.dotClass]" />
              <h4 class="text-sm font-medium text-foreground/90">{{ column.title }}</h4>
              <div class="ml-auto text-xs tabular-nums font-medium px-2 py-0.5 rounded-md bg-muted text-muted-foreground">
                {{ column.tasks.length }}
              </div>
            </div>

            <!-- Column content (virtual scroller) -->
            <div
              :ref="(el) => { if (el) columnScrollRefs[columnIndex] = el as HTMLElement }"
              class="flex-1 min-h-0 overflow-y-auto scrollbar-auto pr-1 -mr-1"
            >
              <!-- Virtual scroller when tasks exist -->
              <template v-if="column.tasks.length > 0">
                <div
                  :style="{ height: `${getVirtualizer(columnIndex)?.getTotalSize() ?? 0}px` }"
                  class="relative w-full"
                >
                  <div
                    v-for="virtualRow in getVirtualizer(columnIndex)?.getVirtualItems() ?? []"
                    :key="`col-${columnIndex}-${virtualRow.index}`"
                    :data-index="virtualRow.index"
                    :ref="(el: any) => { if (el?.nodeType === 1) getVirtualizer(columnIndex)?.measureElement(el) }"
                    class="absolute left-0 w-full"
                    :style="{ transform: `translateY(${virtualRow.start}px)` }"
                  >
                    <div class="mx-0 mb-2 p-3 rounded-lg border bg-card hover:bg-muted/40 transition-colors group/task">
                      <div class="flex items-start justify-between gap-2">
                        <div class="min-w-0 flex-1 space-y-1.5">
                          <p class="text-sm font-medium truncate text-foreground/90">
                            {{ columnTask(column, virtualRow.index).custom_title || columnTask(column, virtualRow.index).title || columnTask(column, virtualRow.index).id.slice(0, 8) }}
                          </p>
                          <div class="flex items-center gap-2">
                            <Badge :variant="getTaskStatusVariantRaw(columnTask(column, virtualRow.index).status)" class="text-[10px] leading-tight px-1.5 py-0">
                              {{ getTaskStatusLabelRaw(columnTask(column, virtualRow.index).status) }}
                            </Badge>
                            <span class="text-[11px] tabular-nums text-muted-foreground/60">
                              {{ columnTask(column, virtualRow.index).token_count || 0 }} tokens
                            </span>
                          </div>
                        </div>
                        <div class="flex items-center gap-0.5 shrink-0 -mr-0.5">
                          <Button
                            v-if="columnTask(column, virtualRow.index).status === 'done' && columnTask(column, virtualRow.index).has_audio"
                            variant="ghost"
                            size="sm"
                            class="h-7 w-7 p-0 text-muted-foreground hover:text-foreground"
                            title="播放"
                            @click="handlePlayTask(columnTask(column, virtualRow.index))"
                          >
                            <PlayIcon class="w-3.5 h-3.5" />
                          </Button>
                          <Button
                            v-if="canCancelTask(columnTask(column, virtualRow.index).status)"
                            variant="ghost"
                            size="sm"
                            class="h-7 w-7 p-0 text-muted-foreground hover:text-destructive"
                            title="取消"
                            @click="handleCancelTask(columnTask(column, virtualRow.index).id)"
                          >
                            <XCircleIcon class="w-3.5 h-3.5" />
                          </Button>
                          <Button
                            v-if="['failed', 'mergingfailed', 'cancelled'].includes(columnTask(column, virtualRow.index).status)"
                            variant="ghost"
                            size="sm"
                            class="h-7 w-7 p-0 text-muted-foreground hover:text-foreground"
                            title="重试"
                            @click="handleRetryTask(columnTask(column, virtualRow.index).id)"
                          >
                            <RotateCcwIcon class="w-3.5 h-3.5" />
                          </Button>
                          <Button
                            v-if="columnTask(column, virtualRow.index).status === 'queued'"
                            variant="ghost"
                            size="sm"
                            class="h-7 w-7 p-0 text-muted-foreground hover:text-amber-500"
                            title="强制处理"
                            @click="handleForceTask(columnTask(column, virtualRow.index).id)"
                          >
                            <ZapIcon class="w-3.5 h-3.5" />
                          </Button>
                          <Button
                            variant="ghost"
                            size="sm"
                            class="h-7 w-7 p-0 text-muted-foreground hover:text-foreground"
                            title="查看原文"
                            @click="handleViewText(columnTask(column, virtualRow.index))"
                          >
                            <FileTextIcon class="w-3.5 h-3.5" />
                          </Button>
                        </div>
                      </div>

                      <!-- Error message for failed tasks -->
                      <div
                        v-if="['failed', 'mergingfailed'].includes(columnTask(column, virtualRow.index).status)"
                        class="mt-1.5"
                      >
                        <div class="flex items-start gap-1.5">
                          <AlertCircleIcon class="w-3 h-3 text-destructive shrink-0 mt-0.5" />
                          <p class="text-[11px] text-destructive/80 leading-tight">
                            {{ (columnTask(column, virtualRow.index) as any).error || '任务执行失败' }}
                          </p>
                        </div>
                      </div>

                      <!-- Progress bar for active tasks -->
                      <div
                        v-if="isActiveStatus(columnTask(column, virtualRow.index).status)"
                        class="mt-2"
                      >
                        <div class="flex items-center gap-2">
                          <div class="flex-1 h-1.5 bg-muted/60 rounded-full overflow-hidden">
                            <div
                              class="h-full rounded-full transition-all duration-500 bg-primary"
                              :style="{ width: `${getTaskProgress(columnTask(column, virtualRow.index).status)}%` }"
                            />
                          </div>
                          <span class="text-[11px] tabular-nums text-muted-foreground shrink-0 w-8 text-right">
                            {{ getTaskProgress(columnTask(column, virtualRow.index).status) }}%
                          </span>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              </template>

              <!-- Empty column message -->
              <template v-else>
                <div class="flex flex-col items-center justify-center py-10 text-muted-foreground gap-2">
                  <div class="w-8 h-8 rounded-full bg-muted/60 flex items-center justify-center">
                    <component :is="column.icon" class="w-4 h-4 text-muted-foreground/40" />
                  </div>
                  <p class="text-xs text-muted-foreground/60">{{ column.emptyText }}</p>
                </div>
              </template>
            </div>
          </div>
        </div>
      </div>

      <!-- Stats footer -->
      <div class="px-4 sm:px-5 py-2.5 border-t shrink-0 bg-muted/20">
        <div class="flex items-center justify-between text-xs text-muted-foreground">
          <div class="flex items-center gap-4">
            <span class="tabular-nums">
              总 Tokens: <span class="font-medium text-foreground/80">{{ formatTokens(group.total_tokens) }}</span>
            </span>
            <span class="hidden sm:inline tabular-nums">
              创建于: <span class="font-medium text-foreground/80">{{ formatShortDate(group.created_at) }}</span>
            </span>
          </div>
          <div class="flex items-center gap-1.5">
            <span class="w-2 h-2 rounded-full bg-emerald-500/60" />
            <span class="tabular-nums">{{ group.completed_tasks }}/{{ group.total_tasks }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useVirtualizer } from '@tanstack/vue-virtual'
import type { Virtualizer } from '@tanstack/vue-virtual'
import type { GroupSummary, TaskSummary } from '@/api/client'
import { apiV2 } from '@/api/client'
import { useBatchStore } from '@/stores/batch'
import {
  getTaskStatusVariantRaw,
  getTaskStatusLabelRaw,
  isActiveStatus,
  getTaskProgress,
  formatShortDate,
  formatTokens,
} from '@/composables/useStatus'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Skeleton } from '@/components/ui/skeleton'
import {
  ArrowLeft as ArrowLeftIcon,
  Pause as PauseIcon,
  Play as PlayIcon,
  RotateCcw as RotateCcwIcon,
  Download as DownloadIcon,
  Trash2 as Trash2Icon,
  RefreshCw as RefreshCwIcon,
  Loader2 as Loader2Icon,
  CheckCircle2 as CheckCircle2Icon,
  XCircle as XCircleIcon,
  AlertCircle as AlertCircleIcon,
  Clock as ClockIcon,
  List as ListIcon,
  FileText as FileTextIcon,
  Zap as ZapIcon,
} from 'lucide-vue-next'


const props = defineProps<{
  id: string
}>()

const router = useRouter()
const batchStore = useBatchStore()

const initialLoading = ref(true)
const loadingTasks = ref(false)
const refreshing = ref(false)
const downloading = ref(false)
const fetchError = ref<string | null>(null)
let pollingTimer: ReturnType<typeof setInterval> | null = null

// ─── Group from store ────────────────────────────────
const group = computed<GroupSummary | undefined>(() =>
  batchStore.groupMap.get(props.id),
)

// ─── Load on mount ───────────────────────────────────
onMounted(async () => {
  try {
    await batchStore.getGroupDetailWithTasks(props.id, 0, 100)
  } catch (err: any) {
    fetchError.value = err?.message || '加载失败'
  } finally {
    initialLoading.value = false
  }
  if (group.value && isActiveStatus(group.value.status)) {
    startPolling()
  }
})

onUnmounted(() => {
  stopPolling()
})

// ─── Auto-polling for active groups ───────────────────
function startPolling() {
  stopPolling()
  pollingTimer = setInterval(() => {
    if (group.value && isActiveStatus(group.value.status)) {
      handleRefresh()
    }
  }, 10000)
}

function stopPolling() {
  if (pollingTimer) {
    clearInterval(pollingTimer)
    pollingTimer = null
  }
}

watch(
  () => group.value?.status,
  (status) => {
    if (status && isActiveStatus(status)) {
      startPolling()
    } else {
      stopPolling()
    }
  },
)

// ─── Group tasks from store cache ─────────────────────
const groupTasks = computed<TaskSummary[]>(() => {
  return batchStore.getGroupTasks(props.id)
})

// ─── Kanban columns ──────────────────────────────────
interface KanbanColumn {
  id: string
  title: string
  dotClass: string
  emptyText: string
  tasks: TaskSummary[]
  statuses: string[]
  icon: typeof ClockIcon
}

const kanbanColumns = computed<KanbanColumn[]>(() => {
  const tasks = groupTasks.value

  return [
    {
      id: 'queued',
      title: '等待中',
      dotClass: 'bg-amber-500',
      emptyText: '无等待任务',
      statuses: ['pending', 'queued'],
      tasks: tasks.filter((t) => ['pending', 'queued'].includes(t.status)),
      icon: ClockIcon,
    },
    {
      id: 'processing',
      title: '处理中',
      dotClass: 'bg-blue-500 animate-pulse',
      emptyText: '无处理中任务',
      statuses: ['chunking', 'processing', 'merging'],
      tasks: tasks.filter((t) => ['chunking', 'processing', 'merging'].includes(t.status)),
      icon: Loader2Icon,
    },
    {
      id: 'done',
      title: '已完成',
      dotClass: 'bg-emerald-500',
      emptyText: '无已完成任务',
      statuses: ['completed', 'done'],
      tasks: tasks.filter((t) => ['completed', 'done'].includes(t.status)),
      icon: CheckCircle2Icon,
    },
    {
      id: 'failed',
      title: '失败',
      dotClass: 'bg-red-500',
      emptyText: '无失败任务',
      statuses: ['failed', 'mergingfailed', 'cancelled'],
      tasks: tasks.filter((t) => ['failed', 'mergingfailed', 'cancelled'].includes(t.status)),
      icon: AlertCircleIcon,
    },
  ]
})

// ─── Virtual scrolling for columns ────────────────────
const columnScrollRefs = ref<(HTMLElement | null)[]>([null, null, null, null])

const columnVirtualizers: Virtualizer<HTMLElement, Element>[] = []
for (let i = 0; i < 4; i++) {
  const idx = i
  const v = useVirtualizer({
    get count() {
      return kanbanColumns.value[idx]?.tasks.length ?? 0
    },
    getScrollElement: () => columnScrollRefs.value[idx],
    estimateSize: () => 120,
    overscan: 5,
  })
  columnVirtualizers.push(
    // useVirtualizer returns Ref<Virtualizer>, unwrap to Virtualizer instance
    'value' in v ? (v as any).value : v,
  )
}

function getVirtualizer(index: number): Virtualizer<HTMLElement, Element> | undefined {
  return columnVirtualizers[index]
}

function columnTask(column: KanbanColumn, index: number): TaskSummary {
  return column.tasks[index]
}

// ─── UI helpers ──────────────────────────────────────
const statusVariant = computed(() => {
  if (!group.value) return 'secondary' as const
  switch (group.value.status) {
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
  if (!group.value) return ''
  switch (group.value.status) {
    case 'pending': return '等待中'
    case 'queued': return '队列中'
    case 'processing': return '处理中'
    case 'paused': return '已暂停'
    case 'completed': return '已完成'
    case 'failed': return '失败'
    default: return group.value.status
  }
})

const progress = computed(() => {
  if (!group.value || group.value.total_tasks === 0) return 0
  return (group.value.completed_tasks / group.value.total_tasks) * 100
})

// ── All status helpers imported from useStatus composable ──

// ─── Task actions ────────────────────────────────────
function canCancelTask(status: string): boolean {
  return ['pending', 'queued', 'chunking', 'processing', 'merging', 'mergingfailed', 'paused'].includes(status)
}

function handlePlayTask(task: TaskSummary) {
  router.push(`/tasks/${task.id}`)
}

function handleViewText(task: TaskSummary) {
  router.push(`/tasks/${task.id}`)
}

async function handleCancelTask(taskId: string) {
  try {
    await apiV2.cancelTask(taskId)
    await handleRefresh()
  } catch (err) {
    console.error('Failed to cancel task:', err)
  }
}

async function handleRetryTask(taskId: string) {
  try {
    await apiV2.retryTask(taskId)
    await handleRefresh()
  } catch (err) {
    console.error('Failed to retry task:', err)
  }
}

async function handleForceTask(taskId: string) {
  try {
    await batchStore.forceTask(taskId)
    await handleRefresh()
  } catch (err) {
    console.error('Failed to force task:', err)
  }
}

// ─── Group actions ───────────────────────────────────
async function handleRefresh() {
  if (refreshing.value) return
  refreshing.value = true
  try {
    await batchStore.getGroupDetailWithTasks(props.id, 0, 100)
  } catch (err) {
    console.error('Failed to refresh tasks:', err)
  } finally {
    refreshing.value = false
  }
}

async function handlePause() {
  try {
    await batchStore.pauseGroup(props.id)
  } catch (err) {
    console.error('Failed to pause group:', err)
  }
}

async function handleResume() {
  try {
    await batchStore.resumeGroup(props.id)
  } catch (err) {
    console.error('Failed to resume group:', err)
  }
}

async function handleRetry() {
  try {
    await batchStore.retryFailed(props.id)
  } catch (err) {
    console.error('Failed to retry failed tasks:', err)
  }
}

async function handleDownload() {
  if (downloading.value) return
  downloading.value = true
  try {
    await batchStore.downloadGroupAudio(props.id)
  } catch (err) {
    console.error('Failed to download group audio:', err)
  } finally {
    downloading.value = false
  }
}

async function handleDelete() {
  try {
    await batchStore.removeGroup(props.id)
    router.push('/tasks/batch')
  } catch (err) {
    console.error('Failed to delete group:', err)
  }
}
</script>
