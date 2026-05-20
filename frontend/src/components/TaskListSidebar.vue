<template>
  <div class="space-y-3">
    <!-- Loading State -->
    <div v-if="taskStore.loading && taskStore.tasks.length === 0" class="space-y-2">
      <Skeleton class="h-20 w-full" />
      <Skeleton class="h-20 w-full" />
      <Skeleton class="h-20 w-full" />
    </div>

    <!-- Empty State -->
    <div v-else-if="taskStore.tasks.length === 0" class="py-8 text-center text-muted-foreground">
      <p class="text-sm">暂无任务</p>
      <p class="text-xs mt-1">创建合成任务后将在此显示</p>
    </div>

    <!-- Task Items -->
    <div v-else class="space-y-2.5">
      <div
        v-for="task in displayedTasks"
        :key="task.id"
        class="border rounded-lg p-2 sm:p-3 space-y-2 
               hover:bg-accent/50 dark:hover:bg-accent/70 
               active:bg-accent/70 dark:active:bg-accent/90
               transition-colors duration-150"
      >
        <!-- Status and Voice -->
        <div class="flex items-center justify-between gap-2">
          <Badge :variant="getStatusVariant(task.status)" class="text-xs">
            {{ getStatusText(task.status) }}
          </Badge>
          <span class="text-xs text-muted-foreground truncate max-w-[120px] sm:max-w-none">{{ task.voice }}</span>
        </div>

        <!-- Text Preview -->
        <p class="text-xs line-clamp-2 text-muted-foreground">{{ task.text }}</p>

        <!-- Progress Bar -->
        <div v-if="isProcessing(task.status)" class="space-y-1">
          <Progress :value="task.progress * 100" class="h-1.5" />
        </div>

        <!-- Actions -->
        <div class="flex gap-1 pt-1 flex-wrap">
          <Button
            v-if="task.has_audio"
            size="sm"
            variant="ghost"
            class="h-6 sm:h-7 px-2 text-xs"
            @click="playAudio(task.id)"
          >
            播放
          </Button>
          <a
            v-if="task.has_audio"
            :href="api.getAudioUrl(task.id)"
            download
            class="flex-1 min-w-[60px]"
          >
            <Button size="sm" variant="ghost" class="h-6 sm:h-7 px-2 text-xs w-full">
              下载
            </Button>
          </a>
          <Button
            size="sm"
            variant="ghost"
            class="h-6 sm:h-7 px-2 text-xs text-destructive 
                   hover:bg-destructive/15 dark:hover:bg-destructive/30
                   active:bg-destructive/25 dark:active:bg-destructive/40
                   transition-colors"
            @click="handleDelete(task.id)"
          >
            删除
          </Button>
        </div>

        <!-- Error Message -->
        <div v-if="task.error" class="text-xs text-destructive break-words">
          {{ task.error }}
        </div>
      </div>

      <!-- Show More Indicator -->
      <div v-if="taskStore.tasks.length > 20 && !showAllTasks" class="text-center py-2">
        <Button variant="ghost" size="sm" class="text-xs" @click="showAllTasks = true">
          查看全部 {{ taskStore.tasks.length }} 个任务
        </Button>
      </div>
    </div>

    <!-- Refresh Button -->
    <div class="pt-2 border-t">
      <Button 
        variant="outline" 
        size="sm" 
        class="w-full"
        @click="taskStore.loadTasks" 
        :disabled="taskStore.loading"
      >
        刷新列表
      </Button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { toast } from 'vue-sonner'
import { useTaskStore } from '@/stores/task'
import { api, type TaskStatus } from '@/api/client'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import { Skeleton } from '@/components/ui/skeleton'

const taskStore = useTaskStore()
const showAllTasks = ref(false)

// 默认只显示最近 20 个任务
const displayedTasks = computed(() => {
  if (showAllTasks.value) {
    return taskStore.tasks
  }
  return taskStore.tasks.slice(0, 20)
})

function getStatusVariant(status: TaskStatus): 'default' | 'secondary' | 'destructive' | 'outline' {
  const variants: Record<TaskStatus, any> = {
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

function isProcessing(status: TaskStatus): boolean {
  return ['pending', 'queued', 'synthesizing', 'streaming'].includes(status)
}

function playAudio(taskId: string) {
  const audio = new Audio(api.getAudioUrl(taskId))
  audio.play().catch(err => {
    toast.error('播放失败')
    console.error(err)
  })
}

async function handleDelete(taskId: string) {
  if (!confirm('确定要删除此任务吗？')) return
  
  try {
    await taskStore.removeTask(taskId)
    toast.success('任务已删除')
  } catch (error) {
    toast.error('删除失败')
  }
}
</script>
