<template>
  <div 
    class="border rounded-lg p-3 space-y-2 transition-all duration-150"
    :class="{
      'border-primary/50 bg-primary/5 dark:bg-primary/10': mode === 'active',
      'hover:border-primary/30': mode === 'completed',
      'border-destructive/30 bg-destructive/5': mode === 'failed'
    }"
  >
    <!-- 标题行：可编辑 -->
    <div class="flex items-center justify-between gap-2">
      <div 
        class="flex-1 min-w-0 cursor-text"
        @dblclick="startEditTitle"
        title="双击编辑标题"
      >
        <input
          v-if="isEditingTitle"
          ref="titleInputRef"
          v-model="editingTitle"
          class="w-full text-xs font-medium bg-transparent border-b border-primary focus:outline-none"
          @blur="saveTitle"
          @keydown.enter="saveTitle"
          @keydown.esc="cancelEditTitle"
        />
        <span v-else class="text-xs font-medium truncate block">
          {{ task.custom_title || task.id.slice(0, 8) + '...' }}
        </span>
      </div>
      
      <!-- 状态徽章 -->
      <Badge :variant="getStatusVariant(task.status)" class="text-xs shrink-0">
        {{ getStatusText(task.status) }}
      </Badge>
    </div>

    <!-- 时间链路 -->
    <div class="text-[10px] text-muted-foreground flex items-center gap-2 flex-wrap">
      <span>创建: {{ formatTime(task.created_at) }}</span>
      <span v-if="task.completed_at">→ 完成: {{ formatTime(task.completed_at) }}</span>
      <span v-if="task.elapsed_secs" class="text-primary font-medium">
        用时: {{ formatDuration(task.elapsed_secs) }}
      </span>
    </div>

    <!-- 文本预览（可展开） -->
    <div class="relative">
      <p 
        class="text-xs text-muted-foreground line-clamp-2 cursor-pointer hover:text-foreground transition-colors"
        :class="{ 'line-clamp-none': showFullText }"
        @click="showFullText = !showFullText"
      >
        {{ task.text }}
      </p>
      <button 
        v-if="task.text.length > 100"
        class="text-[10px] text-primary hover:underline mt-1"
        @click="showFullText = !showFullText"
      >
        {{ showFullText ? '收起' : '展开全文' }}
      </button>
    </div>

    <!-- 音色信息 -->
    <div class="text-xs text-muted-foreground">
      音色: {{ task.voice }}
    </div>

    <!-- 进行中任务：进度条 + 动效 -->
    <div v-if="mode === 'active'" class="space-y-1">
      <Progress :value="task.progress * 100" class="h-1.5" />
      <div class="flex items-center gap-2 text-[10px] text-muted-foreground">
        <Loader2Icon class="w-3 h-3 animate-spin" />
        <span>{{ Math.round(task.progress * 100) }}%</span>
      </div>
    </div>

    <!-- 操作按钮 -->
    <div class="flex gap-1 pt-1 flex-wrap">
      <Button
        v-if="task.has_audio && mode !== 'active'"
        size="sm"
        variant="outline"
        class="h-7 px-2 text-xs"
        @click="$emit('play', task.id)"
      >
        <PlayIcon class="w-3 h-3 mr-1" />
        播放
      </Button>
      
      <Button
        size="sm"
        variant="ghost"
        class="h-7 px-2 text-xs"
        @click="$emit('reuse', task)"
      >
        <CopyIcon class="w-3 h-3 mr-1" />
        复用
      </Button>
      
      <a
        v-if="task.has_audio"
        :href="api.getAudioUrl(task.id)"
        download
      >
        <Button size="sm" variant="ghost" class="h-7 px-2 text-xs">
          <DownloadIcon class="w-3 h-3" />
        </Button>
      </a>
      
      <Button
        size="sm"
        variant="ghost"
        class="h-7 px-2 text-xs text-destructive hover:bg-destructive/10"
        @click="$emit('delete', task.id)"
      >
        <TrashIcon class="w-3 h-3" />
      </Button>
    </div>

    <!-- 错误信息 -->
    <div v-if="task.error" class="text-xs text-destructive break-words bg-destructive/10 p-2 rounded">
      {{ task.error }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, nextTick } from 'vue'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import { 
  Play as PlayIcon,
  Copy as CopyIcon,
  Download as DownloadIcon,
  Trash as TrashIcon,
  Loader2 as Loader2Icon
} from 'lucide-vue-next'
import { api, type Task, type TaskStatus } from '@/api/client'
import { debounce } from '@/utils'

const props = defineProps<{
  task: Task
  mode: 'active' | 'completed' | 'failed'
}>()

const emit = defineEmits<{
  play: [taskId: string]
  reuse: [task: Task]
  editTitle: [taskId: string, newTitle: string]
  delete: [taskId: string]
}>()

const isEditingTitle = ref(false)
const editingTitle = ref('')
const titleInputRef = ref<HTMLInputElement>()
const showFullText = ref(false)

function startEditTitle() {
  isEditingTitle.value = true
  editingTitle.value = props.task.custom_title || ''
  nextTick(() => titleInputRef.value?.focus())
}

// Debounced save title to avoid frequent API calls
const debouncedSaveTitle = debounce((taskId: string, title: string) => {
  emit('editTitle', taskId, title)
}, 500)

function saveTitle() {
  const trimmedTitle = editingTitle.value.trim()
  debouncedSaveTitle(props.task.id, trimmedTitle)
  isEditingTitle.value = false
}

function cancelEditTitle() {
  isEditingTitle.value = false
}

function formatTime(dateStr: string): string {
  const date = new Date(dateStr)
  return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
}

function formatDuration(secs: number): string {
  if (secs < 60) return `${secs.toFixed(1)}s`
  const mins = Math.floor(secs / 60)
  const remainingSecs = secs % 60
  return `${mins}m ${remainingSecs.toFixed(0)}s`
}

function getStatusVariant(status: TaskStatus): 'default' | 'secondary' | 'destructive' | 'outline' {
  const variants: Record<TaskStatus, 'default' | 'secondary' | 'destructive' | 'outline'> = {
    pending: 'secondary',
    queued: 'secondary',
    synthesizing: 'default',
    streaming: 'default',
    completed: 'default',
    failed: 'destructive',
  }
  return variants[status] || 'secondary'
}

function getStatusText(status: TaskStatus): string {
  const texts: Record<TaskStatus, string> = {
    pending: '等待中',
    queued: '排队中',
    synthesizing: '合成中',
    streaming: '流式加载',
    completed: '已完成',
    failed: '失败',
  }
  return texts[status] || status
}
</script>
