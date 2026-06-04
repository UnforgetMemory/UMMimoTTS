<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, inject } from 'vue'
import { h } from 'vue'
import type { ExpandedState } from '@tanstack/vue-table'
import { useRouter } from 'vue-router'
import { useBatchStore } from '@/stores/batch'
import type { GroupSummary, TaskSummary } from '@/api/client'
import {
  getGroupStatusLabel,
  getGroupStatusVariant,
  getTaskStatusVariantRaw,
  getTaskStatusLabelRaw,
  formatShortDate,
} from '@/composables/useStatus'
import {
  useVueTable,
  FlexRender,
  getCoreRowModel,
  getExpandedRowModel,
  type ColumnDef,
} from '@tanstack/vue-table'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from '@/components/ui/table'
import {
  ChevronDown,
  ChevronRight,
  Layers,
  Plus as PlusIcon,
  Play,
  Pause,
  RotateCcw,
  Download,
  Trash2,
  Loader2,
} from 'lucide-vue-next'

const router = useRouter()
const batchStore = useBatchStore()
const openBatchWizard = inject<() => void>('openBatchWizard', () => {})

// ── Expanded row state & task fetching ──────────────────

const expanded = ref<ExpandedState>({})
const expandedGroupTasks = ref<Map<string, TaskSummary[]>>(new Map())
const loadingTasks = ref<Set<string>>(new Set())

let mounted = true
const stopExpandedWatcher = watch(expanded, async (newExpanded) => {
  const expandedIds = Object.keys(newExpanded as Record<string, boolean>)
  for (const groupId of expandedIds) {
    if (!expandedGroupTasks.value.has(groupId) && !loadingTasks.value.has(groupId)) {
      loadingTasks.value = new Set([...loadingTasks.value, groupId])
      try {
        const result = await batchStore.getGroupDetailWithTasks(groupId)
        if (!mounted) return
        expandedGroupTasks.value = new Map(expandedGroupTasks.value).set(groupId, result.tasks.items)
      } catch (err) {
        console.error('Failed to load group tasks:', err)
        if (!mounted) return
        expandedGroupTasks.value = new Map(expandedGroupTasks.value).set(groupId, [])
      } finally {
        if (mounted) {
          const next = new Set(loadingTasks.value)
          next.delete(groupId)
          loadingTasks.value = next
        }
      }
    }
  }
}, { deep: true })

onUnmounted(() => {
  mounted = false
  stopExpandedWatcher()
})

// ── Table columns ────────────────────────────────────────

const columns: ColumnDef<GroupSummary>[] = [
  {
    id: 'expand',
    header: '',
    cell: ({ row }) => {
      return h('button', {
        class: 'flex items-center justify-center w-8 h-8 cursor-pointer',
        onClick: (e: MouseEvent) => {
          e.stopPropagation()
          row.toggleExpanded()
        },
      }, row.getIsExpanded()
        ? h(ChevronDown, { class: 'w-4 h-4 text-muted-foreground' })
        : h(ChevronRight, { class: 'w-4 h-4 text-muted-foreground' })
      )
    },
    size: 32,
  },
  {
    accessorKey: 'name',
    header: '分组名称',
    cell: ({ row }) => {
      const group = row.original
      return h('div', { class: 'flex items-center gap-2 min-w-0' }, [
        h(Layers, { class: 'w-4 h-4 text-muted-foreground shrink-0' }),
        h('button', {
          class: 'text-sm font-medium text-foreground hover:text-primary truncate cursor-pointer bg-transparent border-none p-0',
          onClick: (e: MouseEvent) => {
            e.stopPropagation()
            router.push(`/groups/${group.id}`)
          },
        }, group.name)
      ])
    },
  },
  {
    accessorKey: 'status',
    header: '状态',
    cell: ({ row }) => {
      const group = row.original
      return h(Badge, {
        variant: getGroupStatusVariant(group.status),
        class: 'shrink-0 text-[10px] leading-tight px-1.5 py-0',
      }, () => getGroupStatusLabel(group.status))
    },
  },
  {
    id: 'tasks',
    header: '任务进度',
    cell: ({ row }) => {
      const group = row.original
      return h('span', { class: 'text-sm tabular-nums' },
        `${group.completed_tasks}/${group.total_tasks}`
      )
    },
  },
  {
    id: 'progress',
    header: '进度',
    cell: ({ row }) => {
      const group = row.original
      const pct = group.total_tasks === 0 ? 0 : (group.completed_tasks / group.total_tasks) * 100
      return h('div', { class: 'flex items-center gap-2 w-24' }, [
        h(Progress, { 'model-value': pct, class: 'flex-1 h-1.5' }),
        h('span', { class: 'text-xs tabular-nums text-muted-foreground shrink-0 w-9 text-right' },
          `${Math.round(pct)}%`)
      ])
    },
  },
  {
    accessorKey: 'created_at',
    header: '创建时间',
    cell: ({ row }) => {
      return h('span', { class: 'text-sm text-muted-foreground tabular-nums' },
        formatShortDate(row.original.created_at))
    },
  },
  {
    id: 'actions',
    header: '',
    cell: ({ row }) => {
      const group = row.original
      const buttons: ReturnType<typeof h>[] = []

      if (group.status === 'processing') {
        buttons.push(
          h(Button, {
            variant: 'ghost',
            size: 'sm',
            class: 'h-7 w-7 p-0 text-muted-foreground hover:text-foreground',
            onClick: (e: MouseEvent) => { e.stopPropagation(); batchStore.pauseGroup(group.id) },
            title: '暂停',
          }, () => h(Pause, { class: 'w-3.5 h-3.5' }))
        )
      }

      if (group.status === 'paused') {
        buttons.push(
          h(Button, {
            variant: 'ghost',
            size: 'sm',
            class: 'h-7 w-7 p-0 text-muted-foreground hover:text-foreground',
            onClick: (e: MouseEvent) => { e.stopPropagation(); batchStore.resumeGroup(group.id) },
            title: '恢复',
          }, () => h(Play, { class: 'w-3.5 h-3.5' }))
        )
      }

      if (group.failed_tasks > 0) {
        buttons.push(
          h(Button, {
            variant: 'ghost',
            size: 'sm',
            class: 'h-7 w-7 p-0 text-muted-foreground hover:text-foreground',
            onClick: (e: MouseEvent) => { e.stopPropagation(); batchStore.retryFailed(group.id) },
            title: '重试失败任务',
          }, () => h(RotateCcw, { class: 'w-3.5 h-3.5' }))
        )
      }

      if (group.completed_tasks > 0) {
        buttons.push(
          h(Button, {
            variant: 'ghost',
            size: 'sm',
            class: 'h-7 w-7 p-0 text-muted-foreground hover:text-foreground',
            disabled: batchStore.downloadingGroupId === group.id,
            onClick: (e: MouseEvent) => { e.stopPropagation(); batchStore.downloadGroupAudio(group.id) },
            title: '下载音频',
          }, () => batchStore.downloadingGroupId === group.id
            ? h(Loader2, { class: 'w-3.5 h-3.5 animate-spin' })
            : h(Download, { class: 'w-3.5 h-3.5' }))
        )
      }

      buttons.push(
        h(Button, {
          variant: 'ghost',
          size: 'sm',
          class: 'h-7 w-7 p-0 text-muted-foreground hover:text-destructive',
          onClick: (e: MouseEvent) => { e.stopPropagation(); batchStore.removeGroup(group.id) },
          title: '删除',
        }, () => h(Trash2, { class: 'w-3.5 h-3.5' }))
      )

      return h('div', { class: 'flex items-center gap-0.5' }, buttons)
    },
  },
]

// ── Table instance ────────────────────────────────────────

const table = useVueTable({
  get data() { return batchStore.allGroups },
  columns,
  state: {
    get expanded() { return expanded.value },
  },
  onExpandedChange: (updater: ExpandedState | ((old: ExpandedState) => ExpandedState)) => {
    expanded.value = typeof updater === 'function' ? updater(expanded.value) : updater
  },
  getCoreRowModel: getCoreRowModel(),
  getExpandedRowModel: getExpandedRowModel(),
})

// ── Pagination ──────────────────────────────────────────

const totalPages = computed(() => Math.ceil(batchStore.totalCount / batchStore.perPage))

function goToPrevPage() {
  if (batchStore.currentPage > 0) {
    batchStore.loadPage(batchStore.currentPage - 1)
  }
}

function goToNextPage() {
  if (batchStore.hasMore) {
    batchStore.loadPage(batchStore.currentPage + 1)
  }
}

// ── Mount ────────────────────────────────────────────────

onMounted(() => {
  batchStore.loadGroups()
})
</script>

<template>
  <Card class="h-full flex flex-col w-full max-w-[1600px] mx-auto">
    <!-- Toolbar -->
    <div class="flex items-center justify-between px-4 sm:px-5 py-3 border-b shrink-0">
      <h2 class="text-base sm:text-lg font-semibold tracking-tight text-foreground">批量任务</h2>
      <Button size="sm" class="h-8 text-xs gap-1.5" @click="openBatchWizard()">
        <PlusIcon class="w-4 h-4" />
        <span class="hidden sm:inline">新建批量任务</span>
      </Button>
    </div>
    <CardContent class="flex-1 p-0 overflow-auto">
      <Table>
        <TableHeader>
          <TableRow v-for="headerGroup in table.getHeaderGroups()" :key="headerGroup.id">
            <TableHead
              v-for="header in headerGroup.headers"
              :key="header.id"
              :style="header.getSize() !== 150 ? { width: `${header.getSize()}px` } : undefined"
            >
              <FlexRender
                v-if="!header.isPlaceholder"
                :render="header.column.columnDef.header"
                :props="header.getContext()"
              />
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <!-- Loading state -->
          <template v-if="batchStore.loading">
            <TableRow>
              <TableCell :colspan="columns.length" class="h-24 text-center">
                <div class="flex items-center justify-center text-muted-foreground gap-2">
                  <Loader2 class="w-4 h-4 animate-spin" />
                  <span>加载中...</span>
                </div>
              </TableCell>
            </TableRow>
          </template>

          <!-- Empty state -->
          <template v-else-if="table.getRowModel().rows.length === 0">
            <TableRow>
              <TableCell :colspan="columns.length" class="h-24 text-center">
                <div class="flex flex-col items-center justify-center text-muted-foreground gap-2">
                  <Layers class="w-8 h-8" />
                  <p>暂无批量任务</p>
                </div>
              </TableCell>
            </TableRow>
          </template>

          <!-- Data rows -->
          <template v-else>
            <template v-for="row in table.getRowModel().rows" :key="row.id">
              <TableRow
                :data-state="row.getIsSelected() ? 'selected' : undefined"
              >
                <TableCell
                  v-for="cell in row.getVisibleCells()"
                  :key="cell.id"
                  :style="cell.column.getSize() !== 150 ? { width: `${cell.column.getSize()}px` } : undefined"
                >
                  <FlexRender
                    :render="cell.column.columnDef.cell"
                    :props="cell.getContext()"
                  />
                </TableCell>
              </TableRow>

              <!-- Expanded row: task sub-table -->
              <TableRow v-if="row.getIsExpanded()">
                <TableCell :colspan="row.getVisibleCells().length" class="p-0">
                  <div class="bg-muted/30 border-t">
                    <!-- Loading tasks -->
                    <div
                      v-if="loadingTasks.has(row.original.id)"
                      class="flex items-center justify-center py-8 text-muted-foreground gap-2"
                    >
                      <Loader2 class="w-4 h-4 animate-spin" />
                      <span class="text-sm">加载任务中...</span>
                    </div>

                    <!-- Task sub-table -->
                    <div
                      v-else-if="(expandedGroupTasks.get(row.original.id) ?? []).length > 0"
                      class="px-4 py-3"
                    >
                      <table class="w-full text-sm">
                        <thead>
                          <tr class="border-b">
                            <th class="h-8 px-2 text-left align-middle font-medium text-muted-foreground">
                              任务 ID
                            </th>
                            <th class="h-8 px-2 text-left align-middle font-medium text-muted-foreground">
                              状态
                            </th>
                            <th class="h-8 px-2 text-left align-middle font-medium text-muted-foreground">
                              进度
                            </th>
                            <th class="h-8 px-2 text-left align-middle font-medium text-muted-foreground">
                              创建时间
                            </th>
                          </tr>
                        </thead>
                        <tbody>
                          <tr
                            v-for="task in expandedGroupTasks.get(row.original.id)"
                            :key="task.id"
                            class="border-b last:border-0 hover:bg-muted/50 cursor-pointer"
                            @click="router.push(`/tasks/${task.id}`)"
                          >
                            <td class="p-2 align-middle">
                              <span class="text-sm font-mono">{{ task.id.slice(0, 8) }}</span>
                            </td>
                            <td class="p-2 align-middle">
                              <Badge
                                :variant="getTaskStatusVariantRaw(task.status)"
                                class="text-[10px] leading-tight px-1.5 py-0"
                              >
                                {{ getTaskStatusLabelRaw(task.status) }}
                              </Badge>
                            </td>
                            <td class="p-2 align-middle">
                              <div class="flex items-center gap-2 w-24">
                                <Progress :model-value="task.progress" class="flex-1 h-1.5" />
                                <span class="text-xs tabular-nums text-muted-foreground shrink-0 w-9 text-right">
                                  {{ task.progress }}%
                                </span>
                              </div>
                            </td>
                            <td class="p-2 align-middle">
                              <span class="text-sm text-muted-foreground tabular-nums">
                                {{ formatShortDate(task.created_at) }}
                              </span>
                            </td>
                          </tr>
                        </tbody>
                      </table>
                    </div>

                    <!-- Empty tasks -->
                    <div
                      v-else
                      class="flex items-center justify-center py-8 text-muted-foreground"
                    >
                      <span class="text-sm">暂无任务</span>
                    </div>
                  </div>
                </TableCell>
              </TableRow>
            </template>
          </template>
        </TableBody>
      </Table>
    </CardContent>

    <!-- Pagination footer -->
    <div class="flex items-center justify-between px-4 py-3 border-t shrink-0">
      <div class="text-sm text-muted-foreground">
        共 <span class="tabular-nums font-medium text-foreground/80">{{ batchStore.totalCount }}</span> 个分组
      </div>
      <div class="flex items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          :disabled="batchStore.currentPage === 0"
          @click="goToPrevPage"
        >
          上一页
        </Button>
        <span class="text-sm text-muted-foreground tabular-nums">
          {{ batchStore.currentPage + 1 }} / {{ totalPages || 1 }}
        </span>
        <Button
          variant="outline"
          size="sm"
          :disabled="!batchStore.hasMore"
          @click="goToNextPage"
        >
          下一页
        </Button>
      </div>
    </div>
  </Card>
</template>
