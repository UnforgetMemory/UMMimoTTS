<template>
  <div class="space-y-3">
    <!-- 标题栏 -->
    <div class="flex items-center justify-between px-1">
      <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">任务列表</h2>
      <Button variant="ghost" size="sm" @click="taskStore.fetchTasks(0)" :disabled="taskStore.refreshing">
        <Loader2 v-if="taskStore.refreshing" class="w-3 h-3 animate-spin mr-1" />
        <RefreshCw v-else class="w-3 h-3 mr-1" />
        <span class="text-xs">刷新</span>
      </Button>
    </div>

    <!-- 加载骨架屏 -->
    <div v-if="taskStore.loading" class="space-y-2">
      <div v-for="n in 5" :key="n" class="glass-card rounded-xl p-3 animate-pulse">
        <div class="flex items-center justify-between">
          <div class="flex-1 space-y-2">
            <div class="h-4 bg-muted rounded w-1/3"></div>
            <div class="h-3 bg-muted/50 rounded w-1/2"></div>
          </div>
          <div class="w-8 h-8 bg-muted rounded-lg"></div>
        </div>
        <div v-if="n <= 2" class="mt-2 h-1.5 bg-muted rounded-full w-full"></div>
      </div>
    </div>

    <!-- 空状态 -->
    <div v-else-if="taskStore.tasks.length === 0" class="text-center py-16 text-muted-foreground">
      <div class="w-16 h-16 mx-auto mb-4 rounded-full bg-muted/30 flex items-center justify-center">
        <ListIcon class="w-8 h-8 text-muted-foreground/50" />
      </div>
      <p class="text-sm font-medium">暂无任务</p>
      <p class="text-xs mt-1">切换到合成标签页开始创建</p>
    </div>

    <!-- 任务列表 -->
    <TransitionGroup v-else name="task-list" tag="div" class="space-y-2">
      <div v-for="task in taskStore.tasks" :key="task.id"
           class="glass-card rounded-xl p-3 cursor-pointer hover:border-primary/30 hover:bg-primary/[0.02] transition-all duration-150 active:scale-[0.99]"
           @click="goToTask(task.id)">
        <div class="flex items-center justify-between gap-3">
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="text-sm font-medium truncate">{{ task.custom_title || `任务 ${task.id.slice(0, 8)}` }}</span>
              <Badge :variant="statusVariant(task.status)" class="text-[10px] h-4 px-1.5 shrink-0">{{ statusText(task.status) }}</Badge>
            </div>
            <div class="flex items-center gap-3 mt-1 text-[11px] text-muted-foreground">
              <span class="flex items-center gap-1"><ClockIcon class="w-3 h-3" />{{ formatDate(task.created_at) }}</span>
              <span v-if="task.voice" class="flex items-center gap-1"><UserIcon class="w-3 h-3" />{{ task.voice }}</span>
            </div>
          </div>
          <div v-if="task.has_audio" class="shrink-0">
            <Button size="icon" variant="ghost" class="h-8 w-8" @click.stop="handlePlay(task)">
              <Play class="w-4 h-4" />
            </Button>
          </div>
        </div>
        <TaskProgress v-if="task.total_chunks && task.total_chunks > 1" :current="task.current_chunk ?? 0" :total="task.total_chunks" class="mt-2" />
      </div>
    </TransitionGroup>

    <!-- 分页 -->
    <div v-if="taskStore.totalPages > 1" class="flex items-center justify-center gap-2 pt-3">
      <Button variant="outline" size="sm" :disabled="taskStore.currentPage === 0" @click="taskStore.fetchTasks(taskStore.currentPage - 1)">
        <ChevronLeftIcon class="w-4 h-4" />
      </Button>
      <span class="text-xs text-muted-foreground tabular-nums min-w-[60px] text-center">
        {{ taskStore.currentPage + 1 }} / {{ taskStore.totalPages }}
      </span>
      <Button variant="outline" size="sm" :disabled="taskStore.currentPage >= taskStore.totalPages - 1" @click="taskStore.fetchTasks(taskStore.currentPage + 1)">
        <ChevronRightIcon class="w-4 h-4" />
      </Button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { Play, Loader2, RefreshCw, List as ListIcon, Clock as ClockIcon, User as UserIcon, ChevronLeft as ChevronLeftIcon, ChevronRight as ChevronRightIcon } from 'lucide-vue-next'
import { useTaskStore } from '@/stores/task'
import { taskApi } from '@/api/tasks'
import { formatDate } from '@/utils/format'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import TaskProgress from '@/components/TaskProgress.vue'
import type { Task } from '@/types/task'

const router = useRouter()
const taskStore = useTaskStore()

function statusText(status: string) {
  const map: Record<string, string> = { pending: '等待中', queued: '排队中', chunking: '分片中', processing: '合成中', merging: '合并中', done: '已完成', failed: '失败', cancelled: '已取消' }
  return map[status] || status
}
function statusVariant(status: string) {
  if (status === 'done') return 'default'
  if (status === 'failed' || status === 'cancelled') return 'destructive'
  if (status === 'processing' || status === 'merging' || status === 'chunking') return 'default'
  return 'secondary'
}
function goToTask(id: string) { router.push(`/task/${id}`) }
function handlePlay(task: Task) { window.open(taskApi.getAudioUrl(task.id), '_blank') }

onMounted(() => { if (taskStore.tasks.length === 0) taskStore.fetchTasks(0) })
</script>

<style scoped>
.task-list-enter-active { transition: all 0.25s ease-out; }
.task-list-leave-active { transition: all 0.15s ease-in; }
.task-list-enter-from { opacity: 0; transform: translateY(-4px); }
.task-list-leave-to { opacity: 0; transform: scale(0.97); }
.task-list-move { transition: transform 0.2s ease; }
</style>
