<template>
  <div class="space-y-4">
    <!-- Header with refresh button -->
    <div class="flex items-center justify-between mb-2">
      <h2 class="text-lg font-semibold">任务历史</h2>
      <Button 
        variant="outline" 
        size="sm"
        @click="refreshTasks" 
        :disabled="taskStore.loading"
      >
        <Loader2Icon v-if="taskStore.loading" class="w-4 h-4 mr-2 animate-spin" />
        {{ taskStore.loading ? '刷新中...' : '刷新' }}
      </Button>
    </div>

    <!-- Search Bar -->
    <div class="relative">
      <SearchIcon class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
      <Input
        v-model="searchQuery"
        placeholder="搜索任务名称或ID..."
        class="pl-9 text-sm"
      />
    </div>

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

    <!-- Task Items - Partitioned by Status -->
    <div v-else class="space-y-4">
      <!-- Pending Tasks Section -->
      <section v-if="filteredPendingTasks.length > 0" class="space-y-2">
        <h3 class="text-sm font-semibold text-primary flex items-center gap-2">
          <Loader2Icon class="w-4 h-4 animate-spin" />
          进行中 ({{ filteredPendingTasks.length }})
        </h3>
        <div class="space-y-2">
          <TaskItem 
            v-for="task in filteredPendingTasks" 
            :key="task.id"
            :task="task"
            mode="active"
            @play="handleOpenPlayer"
            @reuse="handleReuseConfig"
            @edit-title="handleEditTitle"
            @delete="handleDelete"
          />
        </div>
      </section>

      <!-- Completed Tasks Section -->
      <section v-if="filteredCompletedTasks.length > 0" class="space-y-2">
        <h3 class="text-sm font-semibold text-muted-foreground">
          已完成 ({{ filteredCompletedTasks.length }})
        </h3>
        <div class="space-y-2">
          <TaskItem 
            v-for="task in filteredCompletedTasks" 
            :key="task.id"
            :task="task"
            mode="completed"
            @play="handleOpenPlayer"
            @reuse="handleReuseConfig"
            @edit-title="handleEditTitle"
            @delete="handleDelete"
          />
        </div>
      </section>

      <!-- Failed Tasks Section (Collapsible) -->
      <section v-if="filteredFailedTasks.length > 0" class="space-y-2">
        <details class="group">
          <summary class="text-sm font-semibold text-destructive cursor-pointer list-none flex items-center gap-2">
            <ChevronRightIcon class="w-4 h-4 transition-transform group-open:rotate-90" />
            失败 ({{ filteredFailedTasks.length }})
          </summary>
          <div class="mt-2 space-y-2 pl-6">
            <TaskItem 
              v-for="task in filteredFailedTasks" 
              :key="task.id"
              :task="task"
              mode="failed"
              @reuse="handleReuseConfig"
              @edit-title="handleEditTitle"
              @delete="handleDelete"
            />
          </div>
        </details>
      </section>
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
import { api, type Task } from '@/api/client'
import { handleApiError } from '@/utils/errorHandler'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import TaskItem from './TaskItem.vue'
import { 
  Loader2 as Loader2Icon,
  ChevronRight as ChevronRightIcon,
  Search as SearchIcon
} from 'lucide-vue-next'

const taskStore = useTaskStore()
const searchQuery = ref('')

// Computed properties for partitioned tasks
const pendingTasks = computed(() => 
  taskStore.tasks.filter(t => 
    ['pending', 'queued', 'synthesizing', 'streaming'].includes(t.status)
  )
)

const completedTasks = computed(() => 
  taskStore.tasks.filter(t => t.status === 'completed')
)

const failedTasks = computed(() => 
  taskStore.tasks.filter(t => t.status === 'failed')
)

// Filtered tasks based on search query
const filteredPendingTasks = computed(() => {
  if (!searchQuery.value) return pendingTasks.value
  const query = searchQuery.value.toLowerCase()
  return pendingTasks.value.filter(t => 
    t.custom_title?.toLowerCase().includes(query) ||
    t.id.toLowerCase().includes(query)
  )
})

const filteredCompletedTasks = computed(() => {
  if (!searchQuery.value) return completedTasks.value
  const query = searchQuery.value.toLowerCase()
  return completedTasks.value.filter(t => 
    t.custom_title?.toLowerCase().includes(query) ||
    t.id.toLowerCase().includes(query)
  )
})

const filteredFailedTasks = computed(() => {
  if (!searchQuery.value) return failedTasks.value
  const query = searchQuery.value.toLowerCase()
  return failedTasks.value.filter(t => 
    t.custom_title?.toLowerCase().includes(query) ||
    t.id.toLowerCase().includes(query)
  )
})

const emit = defineEmits<{
  'open-player': [taskId: string]
  'reuse-config': [config: { text: string; voice: string | null; model: string }]
}>()

async function handleOpenPlayer(taskId: string) {
  emit('open-player', taskId)
}

function handleReuseConfig(task: Task) {
  emit('reuse-config', {
    text: task.text,
    voice: task.voice,
    model: task.model,
  })
}

async function handleEditTitle(taskId: string, newTitle: string) {
  try {
    await api.updateTaskTitle(taskId, newTitle)
    toast.success('标题已更新')
    await taskStore.loadTasks() // Reload to get updated data
  } catch (error) {
    handleApiError(error, '更新标题失败')
  }
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

async function refreshTasks() {
  await taskStore.loadTasks()
  toast.success('列表已刷新')
}
</script>
