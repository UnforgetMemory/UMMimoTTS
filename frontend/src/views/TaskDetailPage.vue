<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useTaskStore } from '@/stores/task'
import { useAudioStore } from '@/stores/audio'
import { api, type Task, type TaskStatus } from '@/api/client'
import { getTaskStatusText, getTaskStatusVariant, formatFullTimestamp, formatElapsed } from '@/composables/useStatus'
import { Card, CardHeader, CardContent, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Skeleton } from '@/components/ui/skeleton'
import {
  ArrowLeft as ArrowLeftIcon,
  Download as DownloadIcon,
  Trash2 as Trash2Icon,
  RotateCcw as RotateCcwIcon,
  RefreshCw as RefreshCwIcon,
  Copy as CopyIcon,
  Clock as ClockIcon,
  Loader2 as Loader2Icon,
  CheckCircle2 as CheckCircle2Icon,
  XCircle as XCircleIcon,
  AlertCircle as AlertCircleIcon,
  Play as PlayIcon,
  Pause as PauseIcon,
  Volume2 as Volume2Icon,
  Volume1 as Volume1Icon,
  VolumeX as VolumeXIcon,
} from 'lucide-vue-next'

const props = defineProps<{ id: string }>()

const router = useRouter()
const taskStore = useTaskStore()
const audioStore = useAudioStore()

const task = ref<Task | null>(null)
const fetchError = ref<string | null>(null)
const isRefreshing = ref(false)
const detailLoading = computed(() => !task.value && !fetchError.value)

// ─── Fetch task detail ───────────────────────────────────────────
async function fetchTask() {
  fetchError.value = null
  task.value = null
  try {
    task.value = await taskStore.getTaskDetail(props.id)
  } catch (err) {
    fetchError.value = err instanceof Error ? err.message : '加载任务详情失败'
  }
}

/** Manual refresh — fetches fresh task detail from backend */
async function handleManualRefresh() {
  if (!task.value) return
  isRefreshing.value = true
  try {
    taskStore.taskDetailCache.delete(props.id)
    const fresh = await taskStore.getTaskDetail(props.id)
    task.value = {
      ...task.value,
      status: fresh.status,
      progress: fresh.progress,
      total_chunks: fresh.total_chunks,
      current_chunk: fresh.current_chunk,
      token_count: fresh.token_count,
      char_count: fresh.char_count,
      completed_at: fresh.completed_at,
      elapsed_secs: fresh.elapsed_secs,
      has_audio: fresh.has_audio,
    }
    restartElapsedTimer()
  } catch {
    // Silent — SSE or next tick will catch up
  } finally {
    isRefreshing.value = false
  }
}

async function copyError() {
  if (!task.value?.error) return
  try {
    await navigator.clipboard.writeText(task.value.error)
    errorCopied.value = true
    setTimeout(() => { errorCopied.value = false }, 2000)
  } catch {
    const textarea = document.createElement('textarea')
    textarea.value = task.value.error
    document.body.appendChild(textarea)
    textarea.select()
    document.execCommand('copy')
    document.body.removeChild(textarea)
    errorCopied.value = true
    setTimeout(() => { errorCopied.value = false }, 2000)
  }
}

// ─── Audio player ────────────────────────────────────────────────
const audioUrl = computed(() => task.value ? api.getAudioUrl(task.value.id) : '')
const isCurrentAudioPlaying = computed(() =>
  audioStore.isPlaying && audioStore.currentUrl === audioUrl.value
)
const playbackRates = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2, 3, 6]

// ── Seek state ──────────────────────────────────────────
const isSeeking = ref(false)
const seekValue = ref(0)

function onSeekStart() {
  isSeeking.value = true
  seekValue.value = audioStore.currentTime
}

function onSeekInput(e: Event) {
  seekValue.value = Number((e.target as HTMLInputElement).value)
}

function onSeekEnd() {
  audioStore.seek(seekValue.value)
  isSeeking.value = false
}

function toggleAudio() {
  if (audioUrl.value) audioStore.toggle(audioUrl.value)
}

function formatAudioTime(sec: number): string {
  if (!sec || !isFinite(sec)) return '0:00'
  const m = Math.floor(sec / 60)
  const s = Math.floor(sec % 60)
  return `${m}:${s.toString().padStart(2, '0')}`
}

onMounted(() => {
  fetchTask()
  subscribeSse()
  startRefresh()
  startElapsedTimer()
})

watch(() => props.id, (newId) => {
  fetchTask()
  subscribeSse(newId)
  startRefresh()
  startElapsedTimer()
})

onUnmounted(() => {
  stopRefresh()
  stopElapsedTimer()
  audioStore.stop()
})

// ─── Auto-refresh & SSE bridge ──────────────────────────────────

const TERMINAL_STATUSES: TaskStatus[] = ['done', 'failed', 'cancelled', 'mergingfailed']

function isTerminal(): boolean {
  return task.value ? TERMINAL_STATUSES.includes(task.value.status) : false
}

/** Ensure SSE subscription is active for current task */
function subscribeSse(taskId?: string) {
  const id = taskId ?? props.id
  if (id) {
    taskStore.subscribeToTaskEvents(id)
  }
}

/** Watch taskMap SSE updates and merge lightweight fields into task ref */
watch(
  () => task.value && taskStore.taskMap.get(task.value.id),
  (summary) => {
    if (!summary || !task.value) return
    // SSE 只推送状态和进度，不会包含 content 等重数据
    task.value = {
      ...task.value,
      status: summary.status,
      progress: summary.progress,
      total_chunks: summary.total_chunks ?? task.value.total_chunks,
      current_chunk: summary.current_chunk ?? task.value.current_chunk,
      elapsed_secs: summary.elapsed_secs ?? task.value.elapsed_secs,
      has_audio: summary.has_audio ?? task.value.has_audio,
    }
    // If task became active, restart the live timer
    if (!isTerminal()) restartElapsedTimer()
  },
)

// ─── Polling fallback ───────────────────────────────────────────

let refreshTimer: ReturnType<typeof setInterval> | null = null

/** Dynamic interval based on task size – small tasks poll faster */
function getRefreshInterval(): number {
  const t = task.value
  if (!t) return 30000
  const chunks = t.total_chunks ?? 0
  const textLen = t.text?.length ?? 0
  if (chunks < 5 && textLen < 300)  return 8000
  if (chunks < 20 && textLen < 1000) return 12000
  if (chunks < 100 || textLen < 5000) return 20000
  return 30000
}

/** Lightweight refresh: only merge status/progress fields, NEVER overwrite text */
async function doRefresh() {
  if (!task.value || isTerminal()) {
    stopRefresh()
    return
  }

  try {
    // Bust cache so we get fresh data from backend
    taskStore.taskDetailCache.delete(props.id)
    const fresh = await taskStore.getTaskDetail(props.id)

    // Only merge lightweight display fields – skip text/content
    task.value = {
      ...task.value,
      status: fresh.status,
      progress: fresh.progress,
      total_chunks: fresh.total_chunks,
      current_chunk: fresh.current_chunk,
      token_count: fresh.token_count,
      char_count: fresh.char_count,
      completed_at: fresh.completed_at,
      elapsed_secs: fresh.elapsed_secs,
      has_audio: fresh.has_audio,
    }

    // Re-evaluate interval based on latest task size
    restartRefresh()
    restartElapsedTimer()
  } catch {
    // Silently fail – SSE or next tick will catch up
  }
}

function startRefresh() {
  stopRefresh()
  if (!task.value || isTerminal()) return
  refreshTimer = setInterval(doRefresh, getRefreshInterval())
}

function stopRefresh() {
  if (refreshTimer) {
    clearInterval(refreshTimer)
    refreshTimer = null
  }
}

function restartRefresh() {
  startRefresh()
}

// ─── Live elapsed timer (ticks every second for in-progress tasks) ──
const elapsedTick = ref(0)
let elapsedTimer: ReturnType<typeof setInterval> | null = null

function startElapsedTimer() {
  stopElapsedTimer()
  if (!task.value || isTerminal()) return
  elapsedTimer = setInterval(() => {
    elapsedTick.value++
  }, 1000)
}

function stopElapsedTimer() {
  if (elapsedTimer) {
    clearInterval(elapsedTimer)
    elapsedTimer = null
  }
}

function restartElapsedTimer() {
  stopElapsedTimer()
  startElapsedTimer()
}

/** Live elapsed seconds — ticks up in real time while task is active */
const liveElapsed = computed(() => {
  if (!task.value) return null
  void elapsedTick.value // force recompute on each tick
  // For terminal tasks, show the backend-recorded value
  if (task.value.completed_at || isTerminal()) {
    return task.value.elapsed_secs ?? null
  }
  // For active tasks, compute from created_at for live count-up
  const created = new Date(task.value.created_at).getTime()
  return Math.round((Date.now() - created) / 1000)
})

// ─── Computed display values (fallback when API returns 0) ──────
const displayTokenCount = computed(() => {
  if (!task.value) return 0
  // 后端未归零时使用估算（中文约1字1.5 token）
  if (task.value.token_count > 0) return task.value.token_count
  if (task.value.text) return Math.ceil(task.value.text.length * 1.5)
  return 0
})

const displayCharCount = computed(() => {
  if (!task.value) return 0
  // 后端未归零时直接从文本计算
  if (task.value.char_count > 0) return task.value.char_count
  if (task.value.text) return task.value.text.length
  return 0
})

// ─── Timeline steps ──────────────────────────────────────────────
const timelineSteps = computed(() => {
  if (!task.value) return []
  const s = task.value.status
  const steps = [
    { key: 'pending', label: '等待中', done: true },
    { key: 'queued', label: '排队中', done: true },
    { key: 'processing', label: '合成中', done: true },
    { key: 'done', label: '已完成', done: true },
  ]

  const failedStatuses: TaskStatus[] = ['failed', 'mergingfailed', 'cancelled']
  const isFailed = failedStatuses.includes(s)

  return steps.map((step, i) => {
    if (isFailed) {
      if (s === 'cancelled') {
        // cancelled stops at wherever it was
        return { ...step, state: 'pending' as const }
      }
      // failed/mergingfailed: steps before current are done, current is active/fail
    }

    // 已完成状态 — 所有步骤标记完成
    if (s === 'done') {
      return { ...step, state: 'done' as const }
    }

    const statusOrder: TaskStatus[] = ['pending', 'queued', 'chunking', 'processing', 'merging', 'done']
    const currentIndex = statusOrder.indexOf(s)
    const stepIndex = statusOrder.indexOf(step.key as TaskStatus)

    let state: 'done' | 'active' | 'pending' = 'pending'
    if (isFailed) {
      if (step.key === s || (step.key === 'done' && s === 'mergingfailed')) {
        state = 'active'
      } else if (i < steps.length - 1) {
        state = 'done'
      }
    } else {
      if (stepIndex < currentIndex) {
        state = 'done'
      } else if (stepIndex === currentIndex || (s === 'chunking' && step.key === 'processing') || (s === 'merging' && step.key === 'processing')) {
        state = 'active'
      } else {
        state = 'pending'
      }
    }

    return { ...step, state }
  })
})

// ─── Actions ─────────────────────────────────────────────────────
function goBack() {
  router.back()
}

async function handleRetry() {
  if (!task.value) return
  try {
    await taskStore.retryTask(task.value.id)
    // Re-fetch to update status
    taskStore.taskDetailCache.delete(task.value.id)
    await fetchTask()
  } catch {
    // error handled in store
  }
}

async function handleDelete() {
  if (!task.value) return
  try {
    await taskStore.removeTask(task.value.id)
    router.back()
  } catch {
    // error handled in store
  }
}

function handleReuse() {
  if (!task.value) return
  const t = task.value
  router.push({
    path: '/synthesize',
    query: {
      text: t.text,
      voice: t.voice || '',
      model: t.model,
      context: t.context || '',
    },
  })
}

function downloadAudio() {
  if (!task.value) return
  const a = document.createElement('a')
  a.href = api.getAudioUrl(task.value.id)
  a.download = ''
  a.click()
}

const copied = ref(false)
const errorCopied = ref(false)
async function copyText() {
  if (!task.value) return
  try {
    await navigator.clipboard.writeText(task.value.text)
    copied.value = true
    setTimeout(() => { copied.value = false }, 2000)
  } catch {
    // fallback
    const textarea = document.createElement('textarea')
    textarea.value = task.value.text
    document.body.appendChild(textarea)
    textarea.select()
    document.execCommand('copy')
    document.body.removeChild(textarea)
    copied.value = true
    setTimeout(() => { copied.value = false }, 2000)
  }
}
</script>

<template>
  <div class="h-full overflow-y-auto">
    <!-- Loading skeleton -->
    <div v-if="detailLoading && !task" class="max-w-4xl xl:max-w-5xl 2xl:max-w-6xl mx-auto p-4 sm:p-6 lg:p-8 space-y-6">
      <Skeleton class="h-10 w-48" />
      <Skeleton class="h-64 w-full" />
      <Skeleton class="h-40 w-full" />
      <Skeleton class="h-32 w-full" />
    </div>

    <!-- Error state -->
    <div v-else-if="fetchError" class="max-w-4xl xl:max-w-5xl 2xl:max-w-6xl mx-auto p-4 sm:p-6 lg:p-8">
      <Card>
        <CardContent class="py-12 text-center">
          <AlertCircleIcon class="w-12 h-12 text-destructive mx-auto mb-4" />
          <p class="text-destructive font-medium mb-2">加载失败</p>
          <p class="text-sm text-muted-foreground mb-4">{{ fetchError }}</p>
          <div class="flex justify-center gap-2">
            <Button variant="outline" size="sm" @click="fetchTask">
              <RotateCcwIcon class="w-3.5 h-3.5 mr-1" />
              重试
            </Button>
            <Button variant="outline" size="sm" @click="goBack">
              <ArrowLeftIcon class="w-3.5 h-3.5 mr-1" />
              返回
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>

    <!-- Task not found -->
    <div v-else-if="!task && !detailLoading" class="max-w-4xl xl:max-w-5xl 2xl:max-w-6xl mx-auto p-4 sm:p-6 lg:p-8">
      <Card>
        <CardContent class="py-12 text-center">
          <XCircleIcon class="w-12 h-12 text-muted-foreground mx-auto mb-4" />
          <p class="font-medium mb-2">任务不存在</p>
          <p class="text-sm text-muted-foreground mb-4">任务 {{ id }} 未找到或已被删除</p>
          <Button variant="outline" size="sm" @click="goBack">
            <ArrowLeftIcon class="w-3.5 h-3.5 mr-1" />
            返回
          </Button>
        </CardContent>
      </Card>
    </div>

    <!-- Task detail content -->
    <div v-else-if="task" class="max-w-4xl xl:max-w-5xl 2xl:max-w-6xl mx-auto p-4 sm:p-6 lg:p-8 space-y-6">

      <!-- ═══ Header Card ═══ -->
      <Card>
        <CardHeader class="pb-3">
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2 mb-1">
                <Button
                  variant="ghost"
                  size="icon-xs"
                  class="shrink-0 -ml-1 text-muted-foreground hover:text-foreground"
                  @click="goBack"
                >
                  <ArrowLeftIcon class="w-4 h-4" />
                </Button>
                <!-- Manual refresh button: visible when task is not terminal -->
                <Button
                  v-if="!isTerminal()"
                  variant="ghost"
                  size="icon-xs"
                  class="shrink-0 text-muted-foreground hover:text-foreground"
                  :disabled="isRefreshing"
                  @click="handleManualRefresh"
                >
                  <RefreshCwIcon
                    class="w-4 h-4"
                    :class="{ 'animate-spin': isRefreshing }"
                  />
                </Button>
                <CardTitle class="text-lg truncate">
                  {{ task.custom_title || `任务 ${task.id.slice(0, 8)}` }}
                </CardTitle>
              </div>
              <div class="flex items-center gap-2 ml-8">
                <code class="text-xs text-muted-foreground bg-muted px-1.5 py-0.5 rounded font-mono">
                  {{ task.id.slice(0, 12) }}
                </code>
                <Badge :variant="getTaskStatusVariant(task.status)" class="text-xs">
                  {{ getTaskStatusText(task.status) }}
                </Badge>
              </div>
            </div>
          </div>
          <div class="flex items-center gap-4 mt-2 text-xs text-muted-foreground ml-8">
            <span class="flex items-center gap-1">
              <ClockIcon class="w-3 h-3" />
              创建于 {{ formatFullTimestamp(task.created_at) }}
            </span>
            <span v-if="task.completed_at" class="flex items-center gap-1">
              <CheckCircle2Icon class="w-3 h-3" />
              完成于 {{ formatFullTimestamp(task.completed_at) }}
            </span>
          </div>
        </CardHeader>
      </Card>

      <!-- ═══ Audio Section ═══ -->
      <Card v-if="task.has_audio">
        <CardHeader class="pb-3 flex flex-row items-center justify-between">
          <CardTitle class="text-sm font-medium text-muted-foreground">音频</CardTitle>
          <Button
            variant="outline"
            size="sm"
            class="h-7 text-xs gap-1"
            @click="downloadAudio"
          >
            <DownloadIcon class="w-3.5 h-3.5" />
            下载
          </Button>
        </CardHeader>
        <CardContent>
          <div class="space-y-4">
            <!-- ═══ Progress Bar ═══ -->
            <div class="space-y-1.5">
              <div class="relative h-2">
                <!-- Track background -->
                <div class="absolute inset-0 rounded-full bg-muted" />
                <!-- Filled track (gradient) -->
                <div
                  class="absolute inset-y-0 left-0 rounded-full pointer-events-none"
                  :style="{
                    width: (audioStore.currentUrl === audioUrl ? (isSeeking ? seekValue : audioStore.currentTime) : 0) / (audioStore.duration || 1) * 100 + '%',
                    background: 'linear-gradient(90deg, hsl(var(--primary)) 0%, hsl(var(--primary) / 0.7) 100%)',
                  }"
                />
                <!-- Hidden native range for interaction -->
                <input
                  type="range"
                  min="0"
                  :max="audioStore.duration || 0"
                  step="0.01"
                  :value="audioStore.currentUrl === audioUrl ? (isSeeking ? seekValue : audioStore.currentTime) : 0"
                  @pointerdown="onSeekStart"
                  @input="onSeekInput"
                  @pointerup="onSeekEnd"
                  @pointerleave="onSeekEnd"
                  class="absolute inset-0 w-full h-full appearance-none bg-transparent cursor-pointer z-10
                    [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-4 [&::-webkit-slider-thumb]:h-4
                    [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary
                    [&::-webkit-slider-thumb]:border-[2.5px] [&::-webkit-slider-thumb]:border-background
                    [&::-webkit-slider-thumb]:shadow-md [&::-webkit-slider-thumb]:cursor-pointer
                    [&::-webkit-slider-thumb]:transition-transform [&::-webkit-slider-thumb]:duration-150
                    [&::-webkit-slider-thumb]:hover:scale-125 [&::-webkit-slider-thumb]:active:scale-125
                    [&::-webkit-slider-track]:appearance-none [&::-webkit-slider-track]:bg-transparent
                    [&::-webkit-slider-track]:h-full"
                />
              </div>
              <div class="flex justify-between text-xs text-muted-foreground tabular-nums">
                <span>{{ audioStore.currentUrl === audioUrl ? formatAudioTime(isSeeking ? seekValue : audioStore.currentTime) : '0:00' }}</span>
                <span>{{ audioStore.duration > 0 ? formatAudioTime(audioStore.duration) : '--:--' }}</span>
              </div>
            </div>

            <!-- ═══ Controls Row (centered) ═══ -->
            <div class="flex items-center justify-center gap-4 flex-wrap">
              <!-- Play/Pause -->
              <Button
                size="icon"
                variant="outline"
                class="size-9 shrink-0"
                @click="toggleAudio"
                :disabled="!audioUrl"
              >
                <PlayIcon v-if="!isCurrentAudioPlaying" class="w-4 h-4" />
                <PauseIcon v-else class="w-4 h-4" />
              </Button>

              <!-- Speed Selector -->
              <div class="flex items-center gap-1 flex-wrap justify-center">
                <button
                  v-for="rate in playbackRates"
                  :key="rate"
                  @click="audioStore.changeSpeed(rate)"
                  :class="[
                    'px-2 py-0.5 rounded text-xs font-medium border transition-all duration-150',
                    audioStore.playbackRate === rate
                      ? 'bg-primary text-primary-foreground border-primary shadow-sm scale-105'
                      : 'bg-transparent text-muted-foreground border-border hover:bg-muted hover:text-foreground',
                  ]"
                >
                  {{ rate === 1 ? '1x' : rate === 0.25 ? '0.25x' : `${rate}x` }}
                </button>
              </div>

              <!-- Volume -->
              <div class="flex items-center gap-1.5 shrink-0">
                <button
                  class="text-muted-foreground hover:text-foreground transition-colors p-1"
                  @click="audioStore.toggleMute()"
                  :title="audioStore.isMuted ? '取消静音' : '静音'"
                >
                  <Volume2Icon v-if="audioStore.volume > 0.5 && !audioStore.isMuted" class="w-4 h-4" />
                  <Volume1Icon v-else-if="audioStore.volume > 0 && !audioStore.isMuted" class="w-4 h-4" />
                  <VolumeXIcon v-else class="w-4 h-4" />
                </button>
                <input
                  type="range"
                  min="0"
                  max="1"
                  step="0.05"
                  :value="audioStore.isMuted ? 0 : audioStore.volume"
                  @input="audioStore.changeVolume(Number(($event.target as HTMLInputElement).value))"
                  class="w-16 h-1 rounded-full appearance-none bg-muted cursor-pointer
                    [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3
                    [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary
                    [&::-webkit-slider-thumb]:border-2 [&::-webkit-slider-thumb]:border-background
                    [&::-webkit-slider-thumb]:shadow-sm [&::-webkit-slider-thumb]:cursor-pointer"
                />
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      <!-- ═══ Input Text ═══ -->
      <Card>
        <CardHeader class="pb-2">
          <div class="flex items-center justify-between">
            <CardTitle class="text-sm font-medium text-muted-foreground">合成文本</CardTitle>
            <Button
              variant="ghost"
              size="xs"
              class="h-6 text-xs text-muted-foreground hover:text-foreground -mr-1"
              @click="copyText"
            >
              <CopyIcon class="w-3 h-3 mr-1" />
              {{ copied ? '已复制' : '复制' }}
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          <div class="max-h-48 overflow-y-auto rounded-lg border bg-muted/30 p-3">
            <p class="text-sm leading-relaxed whitespace-pre-wrap break-words text-foreground/90">
              {{ task.text }}
            </p>
          </div>
        </CardContent>
      </Card>

      <!-- ═══ Context ═══ -->
      <Card v-if="task.context">
        <CardHeader class="pb-2">
          <CardTitle class="text-sm font-medium text-muted-foreground">风格描述</CardTitle>
        </CardHeader>
        <CardContent>
          <div class="rounded-lg border bg-muted/30 p-3">
            <p class="text-sm leading-relaxed whitespace-pre-wrap break-words text-foreground/90">
              {{ task.context }}
            </p>
          </div>
        </CardContent>
      </Card>

      <!-- ═══ Config Info ═══ -->
      <Card>
        <CardHeader class="pb-2">
          <CardTitle class="text-sm font-medium text-muted-foreground">配置信息</CardTitle>
        </CardHeader>
        <CardContent>
          <div class="grid grid-cols-2 sm:grid-cols-3 gap-4">
            <div>
              <p class="text-xs text-muted-foreground mb-0.5">模型</p>
              <p class="text-sm font-medium font-mono">{{ task.model || '—' }}</p>
            </div>
            <div>
              <p class="text-xs text-muted-foreground mb-0.5">音色</p>
              <p class="text-sm font-medium font-mono">{{ task.voice || '—' }}</p>
            </div>
            <div>
              <p class="text-xs text-muted-foreground mb-0.5">耗时</p>
              <p class="text-sm font-medium font-mono">{{ formatElapsed(liveElapsed) }}</p>
            </div>
            <div>
              <p class="text-xs text-muted-foreground mb-0.5">Token 数</p>
              <p class="text-sm font-medium font-mono tabular-nums">{{ displayTokenCount.toLocaleString() }}</p>
            </div>
            <div>
              <p class="text-xs text-muted-foreground mb-0.5">字符数</p>
              <p class="text-sm font-medium font-mono tabular-nums">{{ displayCharCount.toLocaleString() }}</p>
            </div>
            <div v-if="task.total_chunks && task.total_chunks > 1">
              <p class="text-xs text-muted-foreground mb-0.5">分片进度</p>
              <p class="text-sm font-medium font-mono tabular-nums">
                {{ task.current_chunk ?? 0 }} / {{ task.total_chunks }}
              </p>
            </div>
          </div>
        </CardContent>
      </Card>

      <!-- ═══ Processing Chain / Timeline ═══ -->
      <Card>
        <CardHeader class="pb-2">
          <CardTitle class="text-sm font-medium text-muted-foreground">处理流程</CardTitle>
        </CardHeader>
        <CardContent>
          <div class="flex items-center gap-0">
            <template v-for="(step, i) in timelineSteps" :key="step.key">
              <!-- Step node -->
              <div class="flex flex-col items-center shrink-0">
                <div
                  :class="[
                    'w-8 h-8 rounded-full flex items-center justify-center border-2 transition-colors',
                    step.state === 'done'
                      ? 'bg-primary/10 border-primary text-primary'
                      : step.state === 'active' && task?.status === 'failed' || task?.status === 'mergingfailed' || task?.status === 'cancelled'
                        ? 'bg-destructive/10 border-destructive text-destructive'
                        : step.state === 'active'
                          ? 'bg-primary border-primary text-primary-foreground'
                          : 'bg-muted border-border text-muted-foreground',
                  ]"
                >
                  <CheckCircle2Icon
                    v-if="step.state === 'done'"
                    class="w-4 h-4"
                  />
                  <Loader2Icon
                    v-else-if="step.state === 'active' && task?.status !== 'failed' && task?.status !== 'mergingfailed' && task?.status !== 'cancelled'"
                    class="w-4 h-4 animate-spin"
                  />
                  <XCircleIcon
                    v-else-if="step.state === 'active'"
                    class="w-4 h-4"
                  />
                  <span v-else class="text-xs font-medium">{{ i + 1 }}</span>
                </div>
                <span
                  :class="[
                    'text-[10px] mt-1.5 whitespace-nowrap',
                    step.state === 'active' ? 'font-medium text-foreground' : 'text-muted-foreground',
                  ]"
                >
                  {{ step.label }}
                </span>
              </div>
              <!-- Connector line -->
              <div
                v-if="i < timelineSteps.length - 1"
                :class="[
                  'flex-1 h-0.5 mx-1 mt-[-18px]',
                  step.state === 'done' ? 'bg-primary/40' : 'bg-border',
                ]"
              />
            </template>
          </div>
        </CardContent>
      </Card>

      <!-- ═══ Error Message (if failed) ═══ -->
      <Card v-if="task.error">
        <CardHeader class="pb-2 flex flex-row items-center justify-between gap-2">
          <CardTitle class="text-sm font-medium text-destructive flex items-center gap-1.5">
            <XCircleIcon class="w-4 h-4" />
            错误信息
          </CardTitle>
          <Button
            variant="ghost"
            size="sm"
            class="h-7 text-xs gap-1 text-muted-foreground hover:text-foreground"
            @click="copyError"
          >
            <CopyIcon class="w-3 h-3" />
            {{ errorCopied ? '已复制' : '复制' }}
          </Button>
        </CardHeader>
        <CardContent>
          <div class="rounded-lg border border-destructive/20 bg-destructive/5 p-3">
            <pre class="text-sm text-destructive/90 whitespace-pre-wrap break-words font-mono leading-relaxed">{{ task.error }}</pre>
          </div>
        </CardContent>
      </Card>

      <!-- ═══ Action Buttons ═══ -->
      <div class="flex items-center gap-2 pb-6">
        <Button
          v-if="task.status === 'failed' || task.status === 'mergingfailed'"
          variant="outline"
          size="sm"
          class="h-8 text-xs"
          @click="handleRetry"
        >
          <RotateCcwIcon class="w-3.5 h-3.5 mr-1" />
          重试
        </Button>
        <Button
          variant="outline"
          size="sm"
          class="h-8 text-xs"
          @click="handleReuse"
        >
          <CopyIcon class="w-3.5 h-3.5 mr-1" />
          复用配置
        </Button>
        <div class="flex-1" />
        <Button
          variant="destructive"
          size="sm"
          class="h-8 text-xs"
          @click="handleDelete"
        >
          <Trash2Icon class="w-3.5 h-3.5 mr-1" />
          删除
        </Button>
      </div>
    </div>
  </div>
</template>
