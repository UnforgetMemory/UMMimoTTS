<template>
  <div class="h-full flex flex-col">
    <!-- Loading skeleton -->
    <div v-if="loading && groups.length === 0" class="p-4 space-y-3">
      <Skeleton class="h-16 w-full rounded-lg" v-for="n in 3" :key="n" />
    </div>

    <!-- Scroll container -->
    <div
      v-else
      ref="scrollContainerRef"
      class="flex-1 overflow-y-auto scrollbar-auto min-h-0"
    >
      <!-- Virtual scroller -->
      <div v-if="virtualRows.length > 0" class="py-1">
        <div :style="{ height: `${virtualizer.getTotalSize()}px` }" class="relative w-full">
          <div
            v-for="virtualRow in virtualizer.getVirtualItems()"
            :key="`gv-${virtualRow.index}`"
            :data-index="virtualRow.index"
            :ref="(el: any) => { if (el?.nodeType === 1) virtualizer.measureElement(el) }"
            class="absolute left-0 w-full px-3 sm:px-4"
            :style="{
              transform: `translateY(${virtualRow.start}px)`,
            }"
          >
            <!-- Section Header -->
            <template v-if="getRowData(virtualRow.index).type === 'section-header'">
              <div
                class="group-section-header flex items-center gap-2.5 py-2 px-3 -mx-3 sm:-mx-4 rounded-none cursor-pointer select-none hover:bg-muted/40 transition-colors"
                :class="sectionHeaderBorder(getRowSection(virtualRow.index))"
                @click="toggleCollapsed(getRowSection(virtualRow.index))"
              >
                <ChevronRightIcon
                  class="w-4 h-4 transition-transform duration-200 shrink-0 text-muted-foreground"
                  :class="{ 'rotate-90': openSections.has(getRowSection(virtualRow.index)) }"
                />
                <span class="text-xs font-semibold tracking-wide text-foreground/80 uppercase">
                  {{ sectionLabel(getRowSection(virtualRow.index)) }}
                </span>
                <span
                  class="text-xs tabular-nums font-medium ml-auto"
                  :class="sectionCountClass(getRowSection(virtualRow.index))"
                >
                  {{ sectionCount(getRowSection(virtualRow.index)) }}
                </span>
              </div>
            </template>

            <!-- Group Card Row -->
            <div v-else-if="getRowData(virtualRow.index).type === 'group'" class="relative py-0.5">
              <GroupCard
                v-if="getRowGroup(virtualRow.index)"
                :group="getRowGroup(virtualRow.index)!"
                :selected="selectedGroupId === getRowGroup(virtualRow.index)?.id"
                @select="(id: string) => $emit('select', id)"
                @pause="(id: string) => $emit('pause', id)"
                @resume="(id: string) => $emit('resume', id)"
                @retry="(id: string) => $emit('retry', id)"
                @cancel="(id: string) => $emit('cancel', id)"
                @delete="(id: string) => $emit('delete', id)"
              />
              <!-- Download overlay button for completed groups -->
              <Button
                v-if="getRowGroup(virtualRow.index)?.status === 'completed' && (getRowGroup(virtualRow.index)?.completed_tasks ?? 0) > 0"
                variant="ghost"
                size="sm"
                class="absolute top-2 right-2 h-6 w-6 p-0 opacity-0 group-hover:opacity-100 transition-opacity"
                @click.stop="$emit('download', getRowGroup(virtualRow.index)!.id)"
                title="下载音频"
              >
                <DownloadIcon class="w-3 h-3" />
              </Button>
            </div>
          </div>
        </div>
      </div>

      <!-- Empty state -->
      <div v-else class="flex flex-col items-center justify-center py-16 text-center px-6">
        <div class="w-12 h-12 rounded-full bg-muted/50 flex items-center justify-center mb-4">
          <FolderIcon class="w-6 h-6 text-muted-foreground/50" />
        </div>
        <p class="text-sm font-medium text-muted-foreground">暂无批量任务</p>
        <p class="text-xs text-muted-foreground/60 mt-1.5">点击上方按钮创建新的批量合成分组</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useVirtualizer } from '@tanstack/vue-virtual'
import type { GroupSummary } from '@/api/client'
import { Skeleton } from '@/components/ui/skeleton'
import { Button } from '@/components/ui/button'
import GroupCard from './GroupCard.vue'
import {
  ChevronRight as ChevronRightIcon,
  Folder as FolderIcon,
  Download as DownloadIcon,
} from 'lucide-vue-next'

// ─── Props ──────────────────────────────────────────
const props = defineProps<{
  groups: GroupSummary[]
  selectedGroupId: string | null
  loading: boolean
}>()

// ─── Emits ──────────────────────────────────────────
defineEmits<{
  select: [groupId: string]
  pause: [groupId: string]
  resume: [groupId: string]
  retry: [groupId: string]
  cancel: [groupId: string]
  delete: [groupId: string]
  download: [groupId: string]
}>()

// ─── Types ──────────────────────────────────────────
type SectionKey = 'processing' | 'pending' | 'completed' | 'failed'

type VirtualRow =
  | { type: 'section-header'; section: SectionKey }
  | { type: 'group'; group: GroupSummary }

// ─── Open/close state ───────────────────────────────
const openSections = ref<Set<SectionKey>>(new Set(['processing', 'pending']))

function toggleCollapsed(section: SectionKey) {
  const newSet = new Set(openSections.value)
  if (newSet.has(section)) {
    newSet.delete(section)
  } else {
    newSet.add(section)
  }
  openSections.value = newSet
}

// ─── Group filtering ────────────────────────────────
const processingGroups = computed(() =>
  props.groups.filter((g) => g.status === 'processing' || g.status === 'paused'),
)

const pendingGroups = computed(() =>
  props.groups.filter((g) => g.status === 'pending' || g.status === 'queued'),
)

const completedGroups = computed(() =>
  props.groups.filter((g) => g.status === 'completed'),
)

const failedGroups = computed(() =>
  props.groups.filter((g) => g.status === 'failed' || g.status === 'cancelled'),
)

// ─── Section helpers ────────────────────────────────
const sectionOrder: SectionKey[] = ['processing', 'pending', 'completed', 'failed']

const sectionLabels: Record<SectionKey, string> = {
  processing: '处理中',
  pending: '排队中',
  completed: '已完成',
  failed: '失败',
}

function sectionLabel(section: SectionKey): string {
  return sectionLabels[section]
}

function sectionCount(section: SectionKey): number {
  switch (section) {
    case 'processing': return processingGroups.value.length
    case 'pending': return pendingGroups.value.length
    case 'completed': return completedGroups.value.length
    case 'failed': return failedGroups.value.length
  }
}

function sectionCountClass(section: SectionKey): string {
  switch (section) {
    case 'processing': return 'text-primary'
    case 'pending': return 'text-muted-foreground'
    case 'completed': return 'text-emerald-500'
    case 'failed': return 'text-destructive'
  }
}

function sectionHeaderBorder(section: SectionKey): string {
  switch (section) {
    case 'processing': return 'border-l-[3px] border-primary/40'
    case 'pending': return 'border-l-[3px] border-muted-foreground/20'
    case 'completed': return 'border-l-[3px] border-emerald-500/40'
    case 'failed': return 'border-l-[3px] border-destructive/40'
  }
}

// ─── Virtual rows ───────────────────────────────────
function getRowData(index: number): VirtualRow {
  return virtualRows.value[index] || { type: 'section-header', section: 'processing' }
}

function getRowGroup(index: number): GroupSummary | null {
  const row = virtualRows.value[index]
  return row?.type === 'group' ? row.group : null
}

function getRowSection(index: number): SectionKey {
  const row = virtualRows.value[index]
  return row?.type === 'section-header' ? row.section : 'processing'
}

const virtualRows = computed<VirtualRow[]>(() => {
  const rows: VirtualRow[] = []

  for (const section of sectionOrder) {
    let sectionGroups: GroupSummary[] = []
    switch (section) {
      case 'processing': sectionGroups = processingGroups.value; break
      case 'pending': sectionGroups = pendingGroups.value; break
      case 'completed': sectionGroups = completedGroups.value; break
      case 'failed': sectionGroups = failedGroups.value; break
    }

    if (sectionGroups.length > 0 || section === 'processing' || section === 'pending') {
      rows.push({ type: 'section-header', section })
      if (openSections.value.has(section)) {
        for (const group of sectionGroups) {
          rows.push({ type: 'group', group })
        }
      }
    }
  }

  return rows
})

// ─── Virtualizer ────────────────────────────────────
const scrollContainerRef = ref<HTMLElement | null>(null)

const virtualizer = useVirtualizer({
  get count() { return virtualRows.value.length },
  getScrollElement: () => scrollContainerRef.value as Element | null,
  estimateSize: (index: number) => {
    const item = virtualRows.value[index]
    if (!item) return 120
    if (item.type === 'section-header') return 32
    return 120
  },
  measureElement: (el: Element) => Math.max(el.getBoundingClientRect().height, 32),
  overscan: 5,
})
</script>
