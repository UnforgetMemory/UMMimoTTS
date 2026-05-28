<template>
  <Card
    class="transition-colors cursor-pointer"
    :class="{ 'border-primary': selected }"
    @click="$emit('select', group.id)"
  >
    <CardContent class="p-3">
      <div class="flex items-start justify-between gap-2">
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
    </CardContent>
  </Card>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { GroupSummary } from '@/api/client'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import {
  Pause as PauseIcon,
  Play as PlayIcon,
  RotateCcw as RotateCcwIcon,
  Trash as TrashIcon,
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

const statusVariant = computed(() => {
  switch (props.group.status) {
    case 'pending': return 'secondary' as const
    case 'queued': return 'secondary' as const
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
    case 'queued': return '队列中'
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

function formatTokens(tokens: number): string {
  if (!tokens) return '0'
  if (tokens >= 1000000) return (tokens / 1000000).toFixed(1) + 'M'
  if (tokens >= 1000) return (tokens / 1000).toFixed(1) + 'K'
  return tokens.toString()
}
</script>
