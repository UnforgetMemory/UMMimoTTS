<script setup lang="ts">
import type { TooltipRootProps } from 'reka-ui'
import type { HTMLAttributes } from 'vue'
import { TooltipRoot, useForwardProps } from 'reka-ui'
import { cn } from '@/lib/utils'

const props = defineProps<TooltipRootProps & {
  class?: HTMLAttributes['class']
}>()

const forwarded = useForwardProps(props)
</script>

<template>
  <TooltipRoot v-bind="forwarded" :class="cn(props.class)">
    <slot />
  </TooltipRoot>
</template>
