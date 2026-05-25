<script setup lang="ts">
import type { TooltipProviderProps } from 'reka-ui'
import type { HTMLAttributes } from 'vue'
import { TooltipProvider as RekaTooltipProvider } from 'reka-ui'
import { cn } from '@/lib/utils'

const props = defineProps<TooltipProviderProps & {
  class?: HTMLAttributes['class']
}>()
</script>

<template>
  <RekaTooltipProvider v-bind="props" :class="cn(props.class)">
    <slot />
  </RekaTooltipProvider>
</template>
