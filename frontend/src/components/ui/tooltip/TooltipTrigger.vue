<script setup lang="ts">
import type { TooltipTriggerProps } from 'reka-ui'
import type { HTMLAttributes } from 'vue'
import { TooltipTrigger as RekaTooltipTrigger } from 'reka-ui'
import { cn } from '@/lib/utils'

const props = defineProps<TooltipTriggerProps & {
  class?: HTMLAttributes['class']
  asChild?: boolean
}>()
</script>

<template>
  <RekaTooltipTrigger v-bind="props" :class="cn(props.class)">
    <slot />
  </RekaTooltipTrigger>
</template>
