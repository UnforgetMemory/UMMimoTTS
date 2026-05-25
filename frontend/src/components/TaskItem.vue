<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import {
  Card,
  CardContent,
  CardFooter,
} from '@/components/ui/card'
import {
  Play as PlayIcon,
  Copy as CopyIcon,
  Download as DownloadIcon,
  Trash as TrashIcon,
  Clock as ClockIcon,
  Mic as MicIcon,
  Hash as HashIcon,
  AlertCircle as AlertCircleIcon,
} from 'lucide-vue-next'
import { api, type Task, type TaskStatus } from '@/api/client'
import type { TaskSummary } from '@/api/client'
import { startTracking, stopTracking, getElapsed } from '@/composables/useElapsedTimer'
import { useTaskStore } from '@/stores/task'

import type { BadgeVariants } from '@/components/ui/badge'

// Only lightweight TaskSummary — full detail fetched lazily on demand
const props = defineProps<{
  task: TaskSummary
  mode: 'active' | 'completed' | 'failed'
}>()

const emit = defineEmits<{
  play: [task: Task]
  reuse: [task: Task]
  editTitle: [taskId: string, newTitle: string]
  delete: [taskId: string]
  'view-text': [task: Task]
}>()

const taskStore = useTaskStore()

// ─── Title editing ────────────────────────────
const isEditingTitle = ref(false)
const editingTitle = ref('')

function startEditTitle() {
  editingTitle.value = props.task.custom_title || ''
  isEditingTitle.value = true
}

function saveTitle() {
  const trimmed = editingTitle.value.trim()
  if (trimmed && trimmed !== props.task.custom_title) {
    emit('editTitle', props.task.id, trimmed)
  }
  isEditingTitle.value = false
}

function cancelEditTitle() {
  isEditingTitle.value = false
}

// ─── Elapsed timer via composable ─────────────
onMounted(() => {
  if (props.mode === 'active') {
    startTracking(props.task.id, props.task.created_at, props.task.completed_at)
  }
})

onUnmounted(() => {
  stopTracking(props.task.id)
})

const taskElapsed = getElapsed(props.task.id)

// ─── Computed ─────────────────────────────────
const displayTime = computed(() => {
  const seconds = taskElapsed.value > 0
    ? taskElapsed.value
    : (props.task.elapsed_secs ?? 0)
  if (seconds < 60) return `${Math.floor(seconds)}s`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${Math.floor(seconds % 60)}s`
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`
})

function getStatusText(status: TaskStatus): string {
  const map: Record<TaskStatus, string> = {
    pending: '等待中',
    queued: '排队中',
    synthesizing: '合成中',
    streaming: '流式加载',
    completed: '已完成',
    failed: '失败',
    cancelled: '已取消',
  }
  return map[status] || status
}

function getStatusVariant(status: TaskStatus): BadgeVariants['variant'] {
  const map: Record<TaskStatus, BadgeVariants['variant']> = {
    pending: 'secondary',
    queued: 'secondary',
    synthesizing: 'default',
    streaming: 'default',
    completed: 'default',
    failed: 'destructive',
    cancelled: 'outline',
  }
  return map[status] || 'secondary'
}

function formatTime(iso: string): string {
  try {
    const date = new Date(iso)
    const now = new Date()
    const isToday = date.toDateString() === now.toDateString()
    const yesterday = new Date(now)
    yesterday.setDate(yesterday.getDate() - 1)
    const isYesterday = date.toDateString() === yesterday.toDateString()

    const time = date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })

    if (isToday) return time
    if (isYesterday) return `昨天 ${time}`
    return `${date.getMonth() + 1}/${date.getDate()} ${time}`
  } catch {
    return iso
  }
}

/** Fetch full Task detail when needed (for play / view-text / reuse) */
async function withFullTask(action: (task: Task) => void) {
  try {
    const full = await taskStore.getTaskDetail(props.task.id)
    action(full)
  } catch (err) {
    console.error('Failed to load task detail:', err)
  }
}

function handlePlay() {
  withFullTask((task) => emit('play', task))
}

function handleReuse() {
  withFullTask((task) => emit('reuse', task))
}

function handleViewText() {
  withFullTask((task) => emit('view-text', task))
}

function downloadAudio(taskId: string) {
  const a = window.document.createElement('a')
  a.href = api.getAudioUrl(taskId)
  a.download = ''
  a.click()
}
</script>

<template>
  <Card
    size="sm"
    :class="{
      'ring-primary/40 bg-primary/[0.03]': mode === 'active',
      'ring-destructive/30 bg-destructive/[0.03]': mode === 'failed',
    }"
  >
    <!-- ===== Header Row: Title + Status Badge ===== -->
    <div class="flex items-center justify-between gap-2 px-3 pt-3">
      <div class="flex-1 min-w-0">
        <div v-if="isEditingTitle" class="flex items-center">
          <input
            v-model="editingTitle"
            class="flex-1 text-sm font-medium bg-transparent border-b border-primary focus:outline-none min-w-0"
            @blur="saveTitle"
            @keydown.enter="saveTitle"
            @keydown.esc="cancelEditTitle"
          />
        </div>
        <button
          v-else
          class="w-full text-left group/title"
          @dblclick="startEditTitle"
          title="双击编辑标题"
        >
          <span class="text-sm font-semibold leading-tight truncate block">
            {{ task.custom_title || '任务_' + task.id.slice(0, 8) }}
          </span>
        </button>
        <div class="text-[10px] text-muted-foreground/40 font-mono truncate select-all mt-0.5" title="任务 ID">
          {{ task.id }}
        </div>
      </div>
      <Badge :variant="getStatusVariant(task.status)" class="text-xs shrink-0">
        {{ getStatusText(task.status) }}
      </Badge>
    </div>

    <!-- ===== Meta Info Bar ===== -->
    <div class="px-3 flex items-center gap-3 text-[11px] text-muted-foreground/60 flex-wrap">
      <span class="inline-flex items-center gap-1">
        <ClockIcon class="w-3 h-3" />
        {{ formatTime(task.created_at) }}
      </span>
      <span v-if="task.voice" class="inline-flex items-center gap-1">
        <MicIcon class="w-3 h-3" />
        {{ task.voice }}
      </span>
      <span v-if="displayTime" class="inline-flex items-center gap-1 text-primary/70 font-medium">
        <HashIcon class="w-3 h-3" />
        {{ displayTime }}
      </span>
    </div>

    <!-- ===== Text Preview / View Text ===== -->
    <CardContent class="!pt-1.5">
      <button
        class="text-xs text-muted-foreground/80 hover:text-foreground transition-colors leading-relaxed cursor-pointer"
        @click="handleViewText"
        title="点击查看原文"
      >
        查看原文
      </button>
    </CardContent>

    <!-- ===== Chunk Progress Indicator ===== -->
    <div v-if="task.total_chunks && task.total_chunks > 1" class="px-3 pt-1">
      <div class="flex items-center gap-2">
        <Progress
          v-if="task.total_chunks"
          :model-value="((task.current_chunk ?? 0) / task.total_chunks) * 100"
          class="h-1 flex-1"
        />
        <span class="text-[10px] text-muted-foreground/60 shrink-0">
          第 {{ task.current_chunk ?? 0 }}/{{ task.total_chunks }} 片
        </span>
      </div>
    </div>

    <!-- ===== Progress Bar (active tasks) ===== -->
    <div v-if="mode === 'active'" class="px-3 pt-1">
      <Progress :model-value="task.progress * 100" class="h-1" />
    </div>

    <!-- ===== Error Message (failed mode — detail fetched lazily) ===== -->
    <div v-if="mode === 'failed'" class="px-3 pt-1">
      <div class="flex items-start gap-1">
        <AlertCircleIcon class="w-3 h-3 text-destructive shrink-0 mt-0.5" />
        <p class="text-[11px] text-destructive/80">任务执行失败</p>
      </div>
    </div>

    <!-- ===== Footer Actions ===== -->
    <CardFooter class="px-2 pt-1 pb-2">
      <div class="flex items-center gap-0.5 w-full">
        <!-- Play Audio -->
        <Button
          v-if="task.has_audio && mode !== 'active'"
          variant="ghost"
          size="sm"
          class="h-7 w-7 p-0"
          title="播放音频"
          @click="handlePlay"
        >
          <PlayIcon class="w-3.5 h-3.5" />
        </Button>

        <!-- Audio Download + Copy for completed -->
        <template v-if="mode === 'completed'">
          <Button
            variant="ghost"
            size="sm"
            class="h-7 w-7 p-0"
            title="下载音频"
            @click="downloadAudio(task.id)"
          >
            <DownloadIcon class="w-3.5 h-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            class="h-7 w-7 p-0"
            title="复用配置"
            @click="handleReuse"
          >
            <CopyIcon class="w-3.5 h-3.5" />
          </Button>
        </template>

        <!-- Spacer -->
        <div class="flex-1" />

        <!-- Char/Token Count -->
        <span class="text-[10px] text-muted-foreground/40 hidden sm:inline">
          {{ task.char_count }}字 · {{ task.token_count }} tokens
        </span>

        <!-- Delete -->
        <Button
          variant="ghost"
          size="sm"
          class="h-7 w-7 p-0 text-muted-foreground/40 hover:text-destructive"
          title="删除任务"
          @click="$emit('delete', task.id)"
        >
          <TrashIcon class="w-3.5 h-3.5" />
        </Button>
      </div>
    </CardFooter>
  </Card>
</template>
