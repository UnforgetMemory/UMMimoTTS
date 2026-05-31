<template>
  <Card
    class="group-card transition-all duration-200 cursor-pointer border"
    :class="[
      selected
        ? 'border-primary shadow-sm shadow-primary/10 ring-1 ring-primary/20'
        : 'hover:border-muted-foreground/20 hover:shadow-sm'
    ]"
    @click="$emit('select', group.id)"
  >
    <CardContent class="p-3">
      <div class="flex items-start justify-between gap-2">
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-2">
            <h4 class="text-sm font-semibold truncate text-foreground">{{ group.name }}</h4>
            <Badge :variant="statusVariant" class="shrink-0 text-[10px] leading-tight px-1.5 py-0">
              {{ statusLabel }}
            </Badge>
          </div>
          <div class="flex items-center gap-3 mt-1.5">
            <span class="text-xs text-muted-foreground/70">
              {{ group.completed_tasks }}/{{ group.total_tasks }}
              <span class="text-muted-foreground/50">任务</span>
            </span>
            <span class="text-xs tabular-nums text-muted-foreground/70">
              {{ formatTokens(group.total_tokens) }}
              <span class="text-muted-foreground/50">tokens</span>
            </span>
            <span v-if="group.failed_tasks > 0" class="text-xs text-destructive font-medium">
              {{ group.failed_tasks }} 失败
            </span>
          </div>
        </div>

        <!-- Actions -->
        <div class="flex items-center gap-0.5 shrink-0 -mr-0.5">
          <Button
            v-if="group.status === 'processing'"
            variant="ghost"
            size="sm"
            class="h-7 w-7 p-0 text-muted-foreground hover:text-foreground"
            @click.stop="$emit('pause', group.id)"
            title="暂停"
          >
            <PauseIcon class="w-3.5 h-3.5" />
          </Button>
          <Button
            v-if="group.status === 'paused'"
            variant="ghost"
            size="sm"
            class="h-7 w-7 p-0 text-muted-foreground hover:text-foreground"
            @click.stop="$emit('resume', group.id)"
            title="恢复"
          >
            <PlayIcon class="w-3.5 h-3.5" />
          </Button>
          <Button
            v-if="group.status === 'failed'"
            variant="ghost"
            size="sm"
            class="h-7 w-7 p-0 text-muted-foreground hover:text-foreground"
            @click.stop="$emit('retry', group.id)"
            title="重试失败任务"
          >
            <RotateCcwIcon class="w-3.5 h-3.5" />
          </Button>
          <Button
            v-if="['pending', 'queued', 'processing', 'paused'].includes(group.status)"
            variant="ghost"
            size="sm"
            class="h-7 w-7 p-0 text-muted-foreground hover:text-destructive"
            @click.stop="$emit('cancel', group.id)"
            title="停止"
          >
            <XCircleIcon class="w-3.5 h-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            class="h-7 w-7 p-0 text-muted-foreground hover:text-destructive"
            @click.stop="$emit('delete', group.id)"
            title="删除"
          >
            <TrashIcon class="w-3.5 h-3.5" />
          </Button>
        </div>
      </div>

      <!-- Progress Bar -->
      <div v-if="group.status === 'processing' || group.status === 'paused'" class="mt-2.5">
        <div class="flex items-center gap-2">
          <div class="flex-1 h-1.5 bg-muted/60 rounded-full overflow-hidden">
            <div
              class="h-full rounded-full transition-all duration-500 ease-out"
              :class="group.status === 'paused' ? 'bg-muted-foreground/40' : 'bg-primary'"
              :style="{ width: `${progress}%` }"
            />
          </div>
          <span class="text-[11px] tabular-nums text-muted-foreground shrink-0 w-9 text-right">
            {{ Math.round(progress) }}%
          </span>
        </div>
      </div>
    </CardContent>
  </Card>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { GroupSummary } from '@/api/client'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Pause as PauseIcon,
  Play as PlayIcon,
  RotateCcw as RotateCcwIcon,
  Trash as TrashIcon,
  XCircle as XCircleIcon,
} from 'lucide-vue-next'

const props = defineProps<{
  group: GroupSummary
  selected?: boolean
}>()

defineEmits<{
  select: [groupId: string]
  pause: [groupId: string]
  resume: [groupId: string]
  retry: [groupId: string]
  cancel: [groupId: string]
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
