<template>
  <div class="h-full flex flex-col">
    <!-- Loading skeleton -->
    <div v-if="loading && groups.length === 0" class="space-y-2 p-3 sm:p-4">
      <Skeleton class="h-20 w-full" />
      <Skeleton class="h-20 w-full" />
      <Skeleton class="h-20 w-full" />
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
                class="flex items-center gap-2 py-2 cursor-pointer select-none group -mx-3 sm:-mx-4 px-3 sm:px-4 hover:bg-muted/30 transition-colors"
                @click="toggleCollapsed(getRowSection(virtualRow.index))"
              >
                <ChevronRightIcon
                  class="w-4 h-4 transition-transform duration-200 shrink-0 text-muted-foreground"
                  :class="{ 'rotate-90': openSections.has(getRowSection(virtualRow.index)) }"
                />
                <!-- Processing section icon -->
                <template v-if="getRowSection(virtualRow.index) === 'processing'">
                  <Loader2Icon
                    v-if="openSections.has('processing')"
                    class="w-4 h-4 animate-spin text-primary shrink-0"
                  />
                  <Loader2Icon v-else class="w-4 h-4 text-muted-foreground shrink-0" />
                </template>
                <!-- Pending section icon -->
                <ClockIcon
                  v-else-if="getRowSection(virtualRow.index) === 'pending'"
                  class="w-4 h-4 text-muted-foreground shrink-0"
                />
                <!-- Completed section icon -->
                <CheckCircle2Icon
                  v-else-if="getRowSection(virtualRow.index) === 'completed'"
                  class="w-4 h-4 text-emerald-500 shrink-0"
                />
                <!-- Failed section icon -->
                <XCircleIcon
                  v-else-if="getRowSection(virtualRow.index) === 'failed'"
                  class="w-4 h-4 text-destructive shrink-0"
                />
                <span class="text-xs font-medium text-foreground">
                  {{ sectionLabel(getRowSection(virtualRow.index)) }}
                </span>
                <span
                  class="text-xs tabular-nums ml-auto"
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
      <div v-else class="flex flex-col items-center justify-center py-12 text-center">
        <FolderIcon class="w-10 h-10 text-muted-foreground/50 mb-3" />
        <p class="text-sm text-muted-foreground">暂无批量任务</p>
        <p class="text-xs text-muted-foreground/70 mt-1">点击上方按钮创建</p>
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
  Loader2 as Loader2Icon,
  Clock as ClockIcon,
  CheckCircle2 as CheckCircle2Icon,
  XCircle as XCircleIcon,
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
    case 'processing': return 'text-primary font-medium'
    case 'pending': return 'text-muted-foreground'
    case 'completed': return 'text-emerald-500 font-medium'
    case 'failed': return 'text-destructive font-medium'
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
