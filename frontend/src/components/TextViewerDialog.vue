<template>
  <Dialog :open="open" @update:open="$emit('update:open', $event)">
    <DialogContent class="sm:max-w-3xl max-h-[85vh] overflow-hidden flex flex-col">
      <DialogHeader class="shrink-0 pr-8">
        <DialogTitle class="flex items-center gap-2">
          <FileTextIcon class="w-4 h-4 shrink-0" />
          <span class="truncate">合成文本</span>
          <span class="text-xs text-muted-foreground font-mono truncate max-w-[240px] shrink-0">
            {{ task?.custom_title || (task?.id ? '任务_' + task.id.slice(0, 8) : '') }}
          </span>
        </DialogTitle>
      </DialogHeader>

      <!-- Virtual Scrolled Text Lines -->
      <div
        ref="scrollContainerRef"
        class="flex-1 overflow-y-auto scrollbar-auto min-h-0 mx-4 mb-4 p-4 bg-muted/30 rounded-lg border"
      >
        <div
          :style="{ height: `${virtualizer.getTotalSize()}px` }"
          class="relative w-full"
        >
          <div
            v-for="virtualRow in virtualizer.getVirtualItems()"
            :key="`l-${virtualRow.index}`"
            :data-index="virtualRow.index"
            :ref="(el: any) => { if (el?.nodeType === 1) virtualizer.measureElement(el) }"
            class="absolute left-0 w-full text-sm leading-relaxed whitespace-pre-wrap break-words px-2"
            :style="{
              transform: `translateY(${virtualRow.start}px)`,
            }"
          >
            {{ lines[virtualRow.index] }}
          </div>
        </div>
      </div>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useVirtualizer } from '@tanstack/vue-virtual'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { FileText as FileTextIcon } from 'lucide-vue-next'
import type { Task } from '@/api/client'

const props = defineProps<{
  open: boolean
  task: Task | null
}>()

defineEmits<{
  'update:open': [value: boolean]
}>()

const LINE_HEIGHT = 28

const lines = computed(() => {
  if (!props.task?.text) return ['']
  return props.task.text.split('\n')
})

const scrollContainerRef = ref<HTMLElement | null>(null)

const virtualizer = useVirtualizer({
  get count() { return lines.value.length },
  getScrollElement: () => scrollContainerRef.value as Element | null,
  estimateSize: () => LINE_HEIGHT,
  overscan: 10,
})
</script>
