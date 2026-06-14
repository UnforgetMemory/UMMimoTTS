<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <h2 class="text-sm font-semibold">任务列表</h2>
      <Button variant="ghost" size="sm" @click="taskStore.fetchTasks(0)" :disabled="taskStore.refreshing">
        <Loader2 v-if="taskStore.refreshing" class="w-3 h-3 animate-spin mr-1" />
        <span v-else>刷新</span>
      </Button>
    </div>

    <div v-if="taskStore.loading" class="space-y-2">
      <Skeleton v-for="n in 3" :key="n" class="h-20 w-full" />
    </div>

    <div v-else-if="taskStore.tasks.length === 0" class="text-center py-8 text-muted-foreground text-sm">
      暂无任务
    </div>

    <div v-else class="space-y-2">
      <Card v-for="task in taskStore.tasks" :key="task.id" class="cursor-pointer hover:border-primary/50 transition-colors" @click="goToTask(task.id)">
        <CardContent class="p-3">
          <div class="flex items-center justify-between">
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2">
                <span class="text-sm font-medium truncate">{{ task.custom_title || `任务 ${task.id.slice(0, 8)}` }}</span>
                <Badge :variant="statusVariant(task.status)" class="text-[10px] h-4 px-1.5 shrink-0">{{ statusText(task.status) }}</Badge>
              </div>
              <div class="flex items-center gap-3 mt-1 text-[11px] text-muted-foreground">
                <span>{{ formatDate(task.created_at) }}</span>
                <span v-if="task.voice">{{ task.voice }}</span>
              </div>
            </div>
            <div v-if="task.has_audio" class="ml-2">
              <Button size="icon" variant="ghost" class="h-7 w-7" @click.stop="handlePlay(task)">
                <Play class="w-3 h-3" />
              </Button>
            </div>
          </div>
          <TaskProgress v-if="task.total_chunks && task.total_chunks > 1" :current="task.current_chunk ?? 0" :total="task.total_chunks" class="mt-2" />
        </CardContent>
      </Card>
    </div>

    <div v-if="taskStore.totalPages > 1" class="flex items-center justify-center gap-2 pt-2">
      <Button variant="outline" size="sm" :disabled="taskStore.currentPage === 0" @click="taskStore.fetchTasks(taskStore.currentPage - 1)">上一页</Button>
      <span class="text-xs text-muted-foreground">{{ taskStore.currentPage + 1 }} / {{ taskStore.totalPages }}</span>
      <Button variant="outline" size="sm" :disabled="taskStore.currentPage >= taskStore.totalPages - 1" @click="taskStore.fetchTasks(taskStore.currentPage + 1)">下一页</Button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { Play, Loader2 } from 'lucide-vue-next'
import { useTaskStore } from '@/stores/task'
import { taskApi } from '@/api/tasks'
import { formatDate } from '@/utils/format'
import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Skeleton } from '@/components/ui/skeleton'
import TaskProgress from '@/components/TaskProgress.vue'
import type { Task } from '@/types/task'

const router = useRouter()
const taskStore = useTaskStore()

function statusText(status: string) {
  const map: Record<string, string> = {
    pending: '等待中', queued: '排队中', chunking: '分片中',
    processing: '合成中', merging: '合并中', done: '已完成',
    failed: '失败', cancelled: '已取消',
  }
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
