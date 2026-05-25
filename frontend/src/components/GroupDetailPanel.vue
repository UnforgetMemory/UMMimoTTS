<template>
  <div class="w-full max-w-4xl mt-8 sm:mt-12">
    <div class="flex flex-col h-full border rounded-lg bg-card shadow-sm">
      <!-- Header -->
      <div class="p-4 border-b">
        <div class="flex items-center justify-between">
          <div class="min-w-0 flex-1">
            <h3 class="text-lg font-semibold truncate">{{ group.name }}</h3>
            <div class="flex items-center gap-2 mt-1">
              <Badge :variant="statusVariant">{{ statusLabel }}</Badge>
              <span class="text-sm text-muted-foreground">
                {{ group.completed_tasks }}/{{ group.total_tasks }} 完成
              </span>
            </div>
          </div>
          <Button
            variant="outline"
            size="sm"
            class="h-8 w-8 p-0 shrink-0 border"
            @click="$emit('close')"
          >
            <XIcon class="w-4 h-4" />
          </Button>
        </div>

        <!-- Progress -->
        <div v-if="group.status === 'processing' || group.status === 'paused'" class="mt-3">
          <Progress :model-value="progress" class="h-2" />
        </div>
      </div>

      <!-- Group Settings -->
      <div class="p-4 border-b space-y-3">
        <h4 class="text-sm font-medium">分组设置</h4>
        <div class="grid grid-cols-2 gap-2 text-sm">
          <div>
            <span class="text-muted-foreground">音色:</span>
            <span class="ml-1">{{ group.voice || '默认' }}</span>
          </div>
          <div>
            <span class="text-muted-foreground">模型:</span>
            <span class="ml-1">{{ group.model }}</span>
          </div>
          <div class="col-span-2" v-if="group.context">
            <span class="text-muted-foreground">上下文:</span>
            <span class="ml-1 line-clamp-2">{{ group.context }}</span>
          </div>
        </div>
      </div>

      <!-- Actions -->
      <div class="p-4 border-b flex gap-2 flex-wrap">
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
          重试失败
        </Button>
        <Button
          v-if="group.completed_tasks > 0"
          variant="outline"
          size="sm"
          :disabled="downloading"
          @click="$emit('download', group.id)"
        >
          <Loader2Icon v-if="downloading" class="w-4 h-4 mr-1 animate-spin" />
          <DownloadIcon v-else class="w-4 h-4 mr-1" />
          {{ downloading ? '打包中...' : '下载全部' }}
        </Button>
      </div>

      <!-- Task List (lazy, virtualized) -->
      <div class="flex-1 flex flex-col min-h-0 overflow-hidden">
        <!-- Loading -->
        <div v-if="loading" class="p-4 space-y-2">
          <Skeleton class="h-10 w-full" />
          <Skeleton class="h-10 w-full" />
          <Skeleton class="h-10 w-full" />
        </div>

        <!-- Empty -->
        <div v-else-if="groupTasks.length === 0" class="flex-1 flex items-center justify-center text-muted-foreground text-sm py-8">
          暂无任务
        </div>

        <!-- Virtual list -->
        <div
          v-else
          ref="scrollContainerRef"
          class="flex-1 overflow-y-auto scrollbar-auto"
        >
          <div :style="{ height: `${virtualizer.getTotalSize()}px` }" class="relative w-full">
            <div
              v-for="virtualRow in virtualizer.getVirtualItems()"
              :key="`vt-${virtualRow.index}`"
              :data-index="virtualRow.index"
              :ref="(el: any) => { if (el?.nodeType === 1) virtualizer.measureElement(el) }"
              class="absolute left-0 w-full"
              :style="{ transform: `translateY(${virtualRow.start}px)` }"
            >
              <template v-if="groupTasks[virtualRow.index]">
              <div
                class="flex items-center justify-between p-2 rounded-md border bg-card hover:bg-muted/30 transition-colors my-1 mx-4"
              >
                <div class="min-w-0 flex-1">
                  <p class="text-sm font-medium truncate">
                    {{ groupTasks[virtualRow.index].custom_title || groupTasks[virtualRow.index].id.slice(0, 8) }}
                  </p>
                  <div class="flex items-center gap-2 mt-0.5">
                    <Badge :variant="taskStatusVariant(groupTasks[virtualRow.index].status)" class="text-xs">
                      {{ taskStatusLabel(groupTasks[virtualRow.index].status) }}
                    </Badge>
                    <template v-if="groupTasks[virtualRow.index].status === 'completed'">
                      <Button
                        v-if="groupTasks[virtualRow.index].has_audio"
                        variant="ghost"
                        size="sm"
                        class="h-7 w-7 p-0"
                        title="播放"
                        @click="handlePlayTask(groupTasks[virtualRow.index])"
                      >
                        <PlayIcon class="w-3.5 h-3.5" />
                      </Button>
                    </template>
                    <Button
                      variant="ghost"
                      size="sm"
                      class="h-7 w-7 p-0"
                      title="查看原文"
                      @click="handleViewTextTask(groupTasks[virtualRow.index])"
                    >
                      <span class="text-xs">原文</span>
                    </Button>
                  </div>
                </div>
                <div v-if="groupTasks[virtualRow.index].status === 'synthesizing' || groupTasks[virtualRow.index].status === 'streaming'" class="ml-2">
                  <span class="text-xs text-muted-foreground">{{ Math.round(groupTasks[virtualRow.index].progress * 100) }}%</span>
                </div>
              </div>
              </template>
            </div>
          </div>

          <!-- Load more / scroll trigger -->
          <div v-if="groupTaskHasMore" class="px-4 py-2 text-center">
            <Button
              variant="outline"
              size="sm"
              class="w-full"
              :disabled="loadingMore"
              @click="loadMoreGroupTasks"
            >
              <Loader2Icon v-if="loadingMore" class="w-4 h-4 animate-spin mr-2" />
              加载更多任务
            </Button>
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
import { ref, computed, watch } from 'vue'
import { useVirtualizer } from '@tanstack/vue-virtual'
import type { GroupSummary, TaskSummary } from '@/api/client'
import { useBatchStore } from '@/stores/batch'
import { useTaskStore } from '@/stores/task'
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
  Loader2 as Loader2Icon,
} from 'lucide-vue-next'
import type { BadgeVariants } from '@/components/ui/badge'

const props = defineProps<{
  group: GroupSummary
  downloading?: boolean
}>()

const emit = defineEmits<{
  close: []
  pause: [groupId: string]
  resume: [groupId: string]
  retry: [groupId: string]
  download: [groupId: string]
  play: [task: TaskSummary]
  'view-text': [task: TaskSummary]
}>()

const batchStore = useBatchStore()
const taskStore = useTaskStore()

const loading = ref(false)
const loadingMore = ref(false)
const scrollContainerRef = ref<HTMLElement | null>(null)

// ─── Group tasks from store cache ─────────────────────
const groupTasks = computed<TaskSummary[]>(() => {
  return batchStore.getGroupTasks(props.group.id)
})

const groupTaskHasMore = computed(() => {
  const cache = batchStore.groupTaskCache.get(props.group.id)
  return cache?.hasMore ?? false
})

// ─── Virtualizer ──────────────────────────────────────

const virtualizer = useVirtualizer({
  get count() { return groupTasks.value.length },
  getScrollElement: () => scrollContainerRef.value as Element | null,
  estimateSize: () => 60,
  measureElement: (el: Element) => Math.max(el.getBoundingClientRect().height, 48),
  overscan: 5,
})

// ─── Infinite scroll: load more when near bottom ──
watch(
  () => virtualizer.value.getVirtualItems(),
  (items) => {
    if (items.length === 0) return
    const lastItem = items[items.length - 1]
    if (lastItem && lastItem.index >= groupTasks.value.length - 3) {
      if (groupTaskHasMore.value && !loadingMore.value) {
        loadMoreGroupTasks()
      }
    }
  },
  { deep: true }
)

// ─── UI helpers ──────────────────────────────────────

const statusVariant = computed(() => {
  switch (props.group.status) {
    case 'pending': return 'secondary' as const
    case 'processing': return 'default' as const
    case 'paused': return 'outline' as const
    case 'completed': return 'secondary' as const
    case 'failed': return 'destructive' as const
    default: return 'secondary' as const
  }
})

const statusLabel = computed(() => {
  switch (props.group.status) {
    case 'pending': return '等待中'
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
    case 'completed': return 'success'
    case 'failed': return 'destructive'
    case 'synthesizing':
    case 'streaming': return 'default'
    case 'queued': return 'warning'
    default: return 'secondary'
  }
}

function taskStatusLabel(status: string): string {
  switch (status) {
    case 'pending': return '等待'
    case 'queued': return '队列'
    case 'synthesizing': return '合成中'
    case 'streaming': return '流式'
    case 'completed': return '完成'
    case 'failed': return '失败'
    case 'cancelled': return '取消'
    default: return status
  }
}

function formatTokens(tokens: number): string {
  if (tokens >= 1000000) return (tokens / 1000000).toFixed(1) + 'M'
  if (tokens >= 1000) return (tokens / 1000).toFixed(1) + 'K'
  return tokens.toString()
}

function formatDate(dateStr: string): string {
  const date = new Date(dateStr)
  return date.toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

// ─── Load tasks (lazy via getGroupDetailWithTasks) ──

async function loadGroupTasksData() {
  loading.value = true
  try {
    // Use the lazy combined endpoint
    await batchStore.getGroupDetailWithTasks(props.group.id, 0, 50)
  } catch (error) {
    console.error('Failed to load group tasks:', error)
  } finally {
    loading.value = false
  }
}

async function loadMoreGroupTasks() {
  loadingMore.value = true
  try {
    const cache = batchStore.groupTaskCache.get(props.group.id)
    await batchStore.loadGroupTasks(props.group.id, (cache?.page ?? -1) + 1)
  } finally {
    loadingMore.value = false
  }
}

// ─── Play / View text ──────────────────────────────

async function handlePlayTask(task: TaskSummary) {
  try {
    await taskStore.getTaskDetail(task.id)
    emit('play', task)
  } catch (error) {
    console.error('Failed to load task detail:', error)
  }
}

async function handleViewTextTask(task: TaskSummary) {
  emit('view-text', task)
}

// ─── Watchers ───────────────────────────────────────

watch(() => props.group.id, loadGroupTasksData, { immediate: true })

// Auto-refresh when processing
let refreshInterval: ReturnType<typeof setInterval> | null = null

watch(() => props.group.status, (status) => {
  if (refreshInterval) {
    clearInterval(refreshInterval)
    refreshInterval = null
  }
  if (status === 'processing') {
    refreshInterval = setInterval(() => {
      batchStore.loadGroupTasks(props.group.id, 0, true)
    }, 5000)
  }
}, { immediate: true })
</script>
