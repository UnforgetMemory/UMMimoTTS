<template>
  <div class="min-h-screen flex flex-col bg-background">
    <div class="flex-1">
      <div class="max-w-4xl mx-auto px-4 py-8">
        <div class="flex items-center gap-3 mb-6">
          <Button variant="ghost" size="sm" @click="router.back()">
            <ArrowLeft class="w-4 h-4" />
          </Button>
          <h1 class="text-lg font-semibold">任务详情</h1>
        </div>

        <div v-if="loading" class="space-y-4">
          <Skeleton class="h-10 w-48" />
          <Skeleton class="h-64 w-full" />
        </div>

        <div v-else-if="error" class="text-center py-12">
          <AlertCircle class="w-12 h-12 text-destructive mx-auto mb-4" />
          <p class="text-destructive font-medium mb-4">{{ error }}</p>
          <Button size="sm" @click="fetchTask">重试</Button>
        </div>

        <div v-else-if="task" class="space-y-6">
          <Card>
            <CardHeader class="pb-3">
              <div class="flex items-start justify-between">
                <div>
                  <CardTitle class="text-lg">{{ task.custom_title || `任务 ${task.id.slice(0, 8)}` }}</CardTitle>
                  <div class="flex items-center gap-2 mt-1">
                    <Badge :variant="statusVariant">{{ statusText }}</Badge>
                    <span class="text-xs text-muted-foreground">{{ formatDate(task.created_at) }}</span>
                  </div>
                </div>
              </div>
            </CardHeader>
            <CardContent class="space-y-4">
              <TaskProgress :current="task.current_chunk ?? 0" :total="task.total_chunks ?? 0" />
              <div class="grid grid-cols-2 sm:grid-cols-3 gap-4 text-sm">
                <div><span class="text-muted-foreground">音色</span><p class="font-medium">{{ task.voice || '—' }}</p></div>
                <div><span class="text-muted-foreground">模型</span><p class="font-medium">{{ task.model || '—' }}</p></div>
                <div><span class="text-muted-foreground">Token</span><p class="font-medium">{{ formatTokens(task.token_count) }}</p></div>
              </div>
            </CardContent>
          </Card>

          <AudioPlayer v-if="task.has_audio" :src="audioUrl" />

          <Card>
            <CardHeader class="pb-2">
              <CardTitle class="text-sm font-medium text-muted-foreground">合成文本</CardTitle>
            </CardHeader>
            <CardContent>
              <div class="max-h-48 overflow-y-auto rounded-lg border bg-muted/30 p-3">
                <p class="text-sm leading-relaxed whitespace-pre-wrap">{{ task.text }}</p>
              </div>
            </CardContent>
          </Card>

          <div v-if="task.error" class="rounded-lg border border-destructive/20 bg-destructive/5 p-3">
            <p class="text-sm text-destructive">{{ task.error }}</p>
          </div>

          <div class="flex items-center gap-2">
            <Button v-if="task.status === 'failed'" size="sm" @click="handleRetry">重试</Button>
            <Button variant="destructive" size="sm" @click="handleDelete">删除</Button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { ArrowLeft, AlertCircle } from 'lucide-vue-next'
import { taskApi } from '@/api/tasks'
import { useTaskStore } from '@/stores/task'
import { formatDate, formatTokens } from '@/utils/format'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Skeleton } from '@/components/ui/skeleton'
import TaskProgress from '@/components/TaskProgress.vue'
import AudioPlayer from '@/components/AudioPlayer.vue'
import type { Task } from '@/types/task'

const props = defineProps<{ id: string }>()
const router = useRouter()
const taskStore = useTaskStore()


const task = ref<Task | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)

const audioUrl = computed(() => task.value ? taskApi.getAudioUrl(task.value.id) : '')
const statusText = computed(() => {
  const map: Record<string, string> = {
    pending: '等待中', queued: '排队中', chunking: '分片中',
    processing: '合成中', merging: '合并中', done: '已完成',
    failed: '失败', cancelled: '已取消', mergingfailed: '合并失败',
  }
  return map[task.value?.status || ''] || task.value?.status || ''
})
const statusVariant = computed(() => {
  const s = task.value?.status || ''
  if (s === 'done') return 'default'
  if (s === 'failed' || s === 'cancelled' || s === 'mergingfailed') return 'destructive'
  if (s === 'processing' || s === 'merging' || s === 'chunking') return 'default'
  return 'secondary'
})

async function fetchTask() {
  loading.value = true
  error.value = null
  try {
    task.value = await taskApi.get(props.id)
  } catch (e: any) {
    error.value = e.message || '加载失败'
  } finally {
    loading.value = false
  }
}

async function handleRetry() {
  if (!task.value) return
  await taskStore.retryTask(task.value.id)
  await fetchTask()
}

async function handleDelete() {
  if (!task.value) return
  await taskStore.deleteTask(task.value.id)
  router.back()
}

onMounted(fetchTask)
</script>
