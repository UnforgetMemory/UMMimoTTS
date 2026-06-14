<template>
  <div v-if="total > 1" class="space-y-1">
    <div class="flex items-center gap-2">
      <Progress :model-value="(current / total) * 100" class="h-1.5 flex-1" />
      <span class="text-[10px] text-muted-foreground tabular-nums shrink-0">
        {{ current }}/{{ total }}
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { Progress } from '@/components/ui/progress'

defineProps<{ current: number; total: number }>()
</script>
