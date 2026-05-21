<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
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
  Loader2 as Loader2Icon,
  Clock as ClockIcon,
  Mic as MicIcon,
  Hash as HashIcon,
  AlertCircle as AlertCircleIcon,
} from 'lucide-vue-next'
import { api, type Task, type TaskStatus } from '@/api/client'

import type { BadgeVariants } from '@/components/ui/badge'

const props = defineProps<{
  task: Task
  mode: 'active' | 'completed' | 'failed'
}>()

const emit = defineEmits<{
  play: [taskId: string]
  reuse: [task: Task]
  editTitle: [taskId: string, newTitle: string]
  delete: [taskId: string]
  'view-text': [task: Task]
}>()

// ─── Title editing ────────────────────────────
const isEditingTitle = ref(false)
const editingTitle = ref('')
const titleInputRef = ref<HTMLInputElement>()

function startEditTitle() {
  editingTitle.value = props.task.custom_title || ''
  isEditingTitle.value = true
  nextTick(() => titleInputRef.value?.focus())
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

// ─── Elapsed timer (active tasks) ─────────────
const currentElapsed = ref(0)
let timer: ReturnType<typeof setInterval> | null = null

function calcElapsed(): number {
  if (!props.task.created_at) return 0
  const start = new Date(props.task.created_at).getTime()
  const end = props.task.completed_at
    ? new Date(props.task.completed_at).getTime()
    : Date.now()
  return Math.max(0, Math.floor((end - start) / 1000))
}

function startTimer() {
  currentElapsed.value = calcElapsed()
  timer = setInterval(() => {
    currentElapsed.value = calcElapsed()
  }, 1000)
}

function stopTimer() {
  if (timer) {
    clearInterval(timer)
    timer = null
  }
}

onMounted(() => {
  if (props.mode === 'active') startTimer()
})

onUnmounted(() => {
  stopTimer()
})

// ─── Computed ─────────────────────────────────
const displayTime = computed(() => {
  const seconds = currentElapsed.value > 0
    ? currentElapsed.value
    : props.task.elapsed_secs || 0
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
            ref="titleInputRef"
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

    <!-- ===== Text Preview ===== -->
    <CardContent class="!pt-1.5">
      <p
        class="text-xs text-muted-foreground/80 line-clamp-2 cursor-pointer hover:text-foreground transition-colors leading-relaxed"
        @click="$emit('view-text', task)"
        title="点击查看全文"
      >
        {{ task.text }}
      </p>
      <button
        v-if="task.text.length > 100"
        class="text-[10px] text-primary/70 hover:text-primary hover:underline mt-1 transition-colors"
        @click.stop="$emit('view-text', task)"
      >
        展开全文
      </button>
    </CardContent>

    <!-- ===== Progress (active only) ===== -->
    <div v-if="mode === 'active'" class="px-3 pb-1">
      <div class="flex items-center gap-2 mb-1">
        <Loader2Icon class="w-3 h-3 animate-spin text-primary" />
        <span class="text-[11px] text-muted-foreground font-medium">{{ Math.round(task.progress * 100) }}%</span>
      </div>
      <Progress :value="task.progress * 100" class="h-1" />
    </div>

    <!-- ===== Footer: Actions ===== -->
    <CardFooter class="gap-1 flex-wrap justify-end">
      <Button
        v-if="task.has_audio && mode !== 'active'"
        size="sm"
        variant="outline"
        class="h-7 px-2.5 text-xs gap-1"
        @click="$emit('play', task.id)"
      >
        <PlayIcon class="w-3 h-3" />
        播放
      </Button>

      <Button
        size="sm"
        variant="ghost"
        class="h-7 px-2 text-xs"
        @click="$emit('reuse', task)"
      >
        <CopyIcon class="w-3 h-3" />
        复用
      </Button>

      <a
        v-if="task.has_audio"
        :href="api.getAudioUrl(task.id)"
        download
      >
        <Button size="sm" variant="ghost" class="h-7 w-7 p-0">
          <DownloadIcon class="w-3.5 h-3.5" />
        </Button>
      </a>

      <Button
        size="sm"
        variant="ghost"
        class="h-7 w-7 p-0 text-destructive/70 hover:text-destructive hover:bg-destructive/10"
        @click="$emit('delete', task.id)"
      >
        <TrashIcon class="w-3.5 h-3.5" />
      </Button>
    </CardFooter>

    <!-- ===== Error ===== -->
    <div
      v-if="task.error"
      class="mx-3 mb-3 flex items-start gap-2 text-xs text-destructive bg-destructive/5 border border-destructive/20 rounded-lg p-2.5"
    >
      <AlertCircleIcon class="w-4 h-4 shrink-0 mt-0.5" />
      <span class="break-words">{{ task.error }}</span>
    </div>
  </Card>
</template>
