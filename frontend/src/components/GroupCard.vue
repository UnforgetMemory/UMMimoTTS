<template>
  <CollapsibleRoot v-model:open="isOpen">
    <Card
      class="transition-colors"
      :class="{ 'border-primary': selected }"
    >
      <CardContent class="p-3">
        <!-- Header: click to select -->
        <div
          class="flex items-start justify-between gap-2 cursor-pointer"
          @click="$emit('select', group.id)"
        >
          <div class="min-w-0 flex-1">
            <h4 class="text-sm font-medium truncate">{{ group.name }}</h4>
            <div class="flex items-center gap-2 mt-1">
              <Badge :variant="statusVariant" class="text-xs">
                {{ statusLabel }}
              </Badge>
              <span class="text-xs text-muted-foreground">
                {{ group.completed_tasks }}/{{ group.total_tasks }}
              </span>
            </div>
          </div>

          <!-- Actions -->
          <div class="flex items-center gap-1 shrink-0">
            <Button
              v-if="group.status === 'processing'"
              variant="ghost"
              size="sm"
              class="h-7 w-7 p-0"
              @click.stop="$emit('pause', group.id)"
              title="暂停"
            >
              <PauseIcon class="w-3.5 h-3.5" />
            </Button>
            <Button
              v-if="group.status === 'paused'"
              variant="ghost"
              size="sm"
              class="h-7 w-7 p-0"
              @click.stop="$emit('resume', group.id)"
              title="恢复"
            >
              <PlayIcon class="w-3.5 h-3.5" />
            </Button>
            <Button
              v-if="group.status === 'failed'"
              variant="ghost"
              size="sm"
              class="h-7 w-7 p-0"
              @click.stop="$emit('retry', group.id)"
              title="重试失败任务"
            >
              <RotateCcwIcon class="w-3.5 h-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              class="h-7 w-7 p-0 text-destructive hover:text-destructive"
              @click.stop="$emit('delete', group.id)"
              title="删除"
            >
              <TrashIcon class="w-3.5 h-3.5" />
            </Button>
          </div>
        </div>

        <!-- Progress Bar -->
        <div v-if="group.status === 'processing' || group.status === 'paused'" class="mt-2">
          <Progress :model-value="progress" class="h-1.5" />
        </div>

        <!-- Failed Count + Tokens row -->
        <div class="mt-1.5 flex items-center justify-between">
          <span v-if="group.failed_tasks > 0" class="text-xs text-destructive">
            {{ group.failed_tasks }} 个失败
          </span>
          <span v-else></span>
          <span class="text-xs text-muted-foreground">
            {{ formatTokens(group.total_tokens) }} tokens
          </span>
        </div>

        <!-- Expand Toggle -->
        <CollapsibleTrigger as-child>
          <Button
            variant="ghost"
            size="sm"
            class="w-full mt-1.5 h-6 text-xs text-muted-foreground justify-center gap-1"
            @click.stop
          >
            <ChevronDownIcon
              class="w-3 h-3 transition-transform duration-200"
              :class="{ 'rotate-180': isOpen }"
            />
            {{ isOpen ? '收起任务' : '展开任务' }}
          </Button>
        </CollapsibleTrigger>

        <!-- Expandable Task List (loaded from batch store cache) -->
        <CollapsibleContent>
          <div class="mt-2 border-t pt-2 space-y-1 max-h-[300px] overflow-y-auto scrollbar-auto">
            <!-- Loading skeleton -->
            <div v-if="tasksLoading" class="space-y-1">
              <Skeleton v-for="i in 3" :key="i" class="h-8 w-full rounded" />
            </div>

            <!-- Empty -->
            <div v-else-if="groupTasks.length === 0" class="text-xs text-muted-foreground text-center py-2">
              暂无任务
            </div>

            <!-- Task list -->
            <div
              v-for="task in groupTasks"
              :key="task.id"
              class="flex items-center gap-2 py-1.5 px-2 rounded text-xs hover:bg-muted/50 transition-colors"
            >
              <!-- Status dot -->
              <span
                class="size-2 rounded-full shrink-0"
                :class="taskStatusColor(task.status)"
                :title="taskStatusLabel(task.status)"
              />

              <!-- Task name -->
              <span class="truncate flex-1 min-w-0">{{ task.custom_title || task.id.slice(0, 8) }}</span>

              <!-- Status badge -->
              <Badge
                :variant="taskStatusVariant(task.status)"
                class="text-[10px] h-4 px-1 shrink-0"
              >
                {{ taskStatusLabel(task.status) }}
              </Badge>
            </div>
          </div>
        </CollapsibleContent>
      </CardContent>
    </Card>
  </CollapsibleRoot>
</template>

<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import type { GroupSummary, TaskSummary } from '@/api/client'
import { useBatchStore } from '@/stores/batch'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { Skeleton } from '@/components/ui/skeleton'
import { CollapsibleRoot, CollapsibleTrigger, CollapsibleContent } from '@/components/ui/collapsible'
import {
  Pause as PauseIcon,
  Play as PlayIcon,
  RotateCcw as RotateCcwIcon,
  Trash as TrashIcon,
  ChevronDown as ChevronDownIcon,
} from 'lucide-vue-next'
import type { BadgeVariants } from '@/components/ui/badge'

const props = defineProps<{
  group: GroupSummary
  selected?: boolean
}>()

defineEmits<{
  select: [groupId: string]
  pause: [groupId: string]
  resume: [groupId: string]
  retry: [groupId: string]
  delete: [groupId: string]
}>()

const batchStore = useBatchStore()
const isOpen = ref(false)
const tasksLoading = ref(false)

// Get cached tasks from batch store
const groupTasks = computed<TaskSummary[]>(() => {
  return batchStore.getGroupTasks(props.group.id)
})

// Lazy load tasks only when first expanded
watch(isOpen, async (open) => {
  if (open) {
    const cache = batchStore.groupTaskCache.get(props.group.id)
    if (!cache || !cache.loaded) {
      tasksLoading.value = true
      try {
        await batchStore.loadGroupTasks(props.group.id, 0)
      } finally {
        tasksLoading.value = false
      }
    }
  }
})

// Refresh tasks when group data updates (new completed/failed counts)
watch(() => [props.group.completed_tasks, props.group.failed_tasks], () => {
  if (isOpen.value) {
    batchStore.loadGroupTasks(props.group.id, 0, true)
  }
})

const statusVariant = computed(() => {
  switch (props.group.status) {
    case 'pending': return 'secondary' as const
    case 'processing': return 'default' as const
    case 'paused': return 'warning' as const
    case 'completed': return 'success' as const
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

function taskStatusColor(status: string): string {
  switch (status) {
    case 'completed': return 'bg-green-500'
    case 'failed': return 'bg-destructive'
    case 'synthesizing':
    case 'streaming': return 'bg-blue-500 animate-pulse'
    case 'queued': return 'bg-yellow-500'
    case 'pending': return 'bg-muted-foreground/40'
    case 'cancelled': return 'bg-muted-foreground/20'
    default: return 'bg-muted-foreground/40'
  }
}

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
</script>
