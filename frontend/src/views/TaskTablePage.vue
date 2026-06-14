<script setup lang="ts">
import { ref, computed, watch, onMounted, h } from 'vue'
import { useRouter } from 'vue-router'
import { toast } from 'vue-sonner'
import {
  FlexRender,
  useVueTable,
  getCoreRowModel,
  getSortedRowModel,
  type ColumnDef,
  type RowSelectionState,
  type SortingState,
} from '@tanstack/vue-table'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { Card, CardContent } from '@/components/ui/card'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog'
import {
  Search,
  Play,
  Download,
  Trash2,
  RotateCcw,
  Copy,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Loader2,
  Inbox,
  Square,
  Eraser,
  AlertTriangle,
  ArrowUp,
  ArrowDown,
  ArrowUpDown,
} from 'lucide-vue-next'
import { useTaskStore } from '@/stores/task'
import { useConfigStore } from '@/stores/config'
import { useAudioStore } from '@/stores/audio'
import { api, type TaskSummary, type TaskStatus } from '@/api/client'
import { debounce } from '@/utils'
import { getTaskStatusText, getTaskStatusVariant, formatLocalDateTime, formatTokens } from '@/composables/useStatus'

const router = useRouter()
const taskStore = useTaskStore()
const configStore = useConfigStore()
const audioStore = useAudioStore()

function getVoiceDisplay(voiceId: string | null): { name: string; gender: string } | null {
  if (!voiceId) return null
  const preset = configStore.voices.find(v => v.id === voiceId)
  return preset ? { name: preset.name, gender: preset.gender } : { name: voiceId, gender: '' }
}

// ── Local state ────────────────────────────────────────────────
const searchQuery = ref('')
const statusFilterValue = ref('all')
const voiceFilter = ref('all')
const providerFilter = ref('all')
const rowSelection = ref<RowSelectionState>({})
const localPageIndex = ref(0)
const localPageSize = ref(50)
const sorting = ref<SortingState>([])
const confirmDeleteTaskId = ref<string | null>(null)
const confirmClearDialogOpen = ref(false)
const isClearingAll = ref(false)
const isBatchDeleting = ref(false)
const playingTaskId = ref<string | null>(null)

// ── Computed ───────────────────────────────────────────────────
const rawTasks = computed<TaskSummary[]>(() => taskStore.allTasks)

const selectedIdsCount = computed(() => Object.keys(rowSelection.value).length)
const selectedTaskIds = computed(() => Object.keys(rowSelection.value))

const availableVoices = computed(() => configStore.voices)
const availableProviders = computed(() => configStore.providers)

const currentlyPlayingId = computed(() => {
  const url = audioStore.currentUrl
  if (!url) return null
  for (const t of rawTasks.value) {
    if (url === api.getAudioUrl(t.id)) return t.id
  }
  return null
})

// Client-side fallback filtering (status + search + voice + provider)
const tasks = computed<TaskSummary[]>(() => {
  const list = rawTasks.value
  const statusFilter = statusFilterValue.value
  const search = searchQuery.value.toLowerCase().trim()
  const voiceF = voiceFilter.value
  const providerF = providerFilter.value
  return list.filter(t => {
    // Status filter
    if (statusFilter !== 'all' && t.status !== statusFilter) return false
    // Voice filter
    if (voiceF !== 'all' && t.voice !== voiceF) return false
    // Provider filter
    if (providerF !== 'all' && t.provider_id !== providerF) return false
    // Search filter (by name or ID)
    if (search) {
      const name = (t.custom_title || t.title || '').toLowerCase()
      if (!name.includes(search) && !t.id.toLowerCase().includes(search)) return false
    }
    return true
  })
})

// Enrich tasks with provider names + token fallback from detail cache
const enrichedTasks = computed<TaskSummary[]>(() => {
  return tasks.value.map(t => {
    const providerName = t.provider_id
      ? configStore.providers.find(p => p.id === t.provider_id)?.name || t.provider_id
      : ''
    // Try to get real token_count from detail cache as fallback for any status
    let displayTokens = t.token_count
    if (displayTokens === 0) {
      const cached = taskStore.taskDetailCache.get(t.id)
      if (cached && cached.token_count > 0) displayTokens = cached.token_count
    }
    return { ...t, _providerName: providerName, _displayTokens: displayTokens } as TaskSummary & { _providerName: string; _displayTokens: number }
  })
})
const totalPages = computed(() => Math.ceil(taskStore.totalCount / localPageSize.value))
const isFirstPage = computed(() => localPageIndex.value === 0)
const isLastPage = computed(() => localPageIndex.value >= totalPages.value - 1)
const isLoading = computed(() => taskStore.loading || taskStore.refreshing)
const isInitialLoad = computed(() => isLoading.value && tasks.value.length === 0)
const isEmpty = computed(() => !isLoading.value && tasks.value.length === 0)
const pageInfo = computed(() => {
  const start = localPageIndex.value * localPageSize.value + 1
  const end = Math.min((localPageIndex.value + 1) * localPageSize.value, taskStore.totalCount)
  return `${start}-${end} / ${taskStore.totalCount}`
})

// ── Debounced search ──────────────────────────────────────────
const debouncedSearch = debounce((query: string) => {
  taskStore.searchTasks(query)
}, 300)

watch(searchQuery, (val) => {
  localPageIndex.value = 0
  debouncedSearch(val)
})

// ── Status filter ─────────────────────────────────────────────
const statusOptions: { value: string; label: string }[] = [
  { value: 'all', label: '全部状态' },
  { value: 'pending', label: '等待中' },
  { value: 'queued', label: '排队中' },
  { value: 'chunking', label: '分片中' },
  { value: 'processing', label: '合成中' },
  { value: 'merging', label: '合并中' },
  { value: 'done', label: '已完成' },
  { value: 'failed', label: '失败' },
  { value: 'cancelled', label: '已取消' },
  { value: 'paused', label: '已暂停' },
]

function onStatusFilterChange(value: unknown) {
  const v = value as string
  statusFilterValue.value = v
  localPageIndex.value = 0
  const status = v === 'all' ? undefined : (v as TaskStatus)
  taskStore.filterByStatus(status)
}

// ── Actions ───────────────────────────────────────────────────
function navigateToTask(taskId: string) {
  router.push(`/tasks/${taskId}`)
}

function handleDownload(taskId: string, e: Event) {
  e.stopPropagation()
  const a = document.createElement('a')
  a.href = api.getAudioUrl(taskId)
  a.download = ''
  a.click()
}

function handleDelete(taskId: string, e: Event) {
  e.stopPropagation()
  confirmDeleteTaskId.value = taskId
}

function confirmDelete() {
  const id = confirmDeleteTaskId.value
  if (!id) return
  taskStore.removeTask(id).catch(err => {
    console.error('Failed to delete task:', err)
  })
  confirmDeleteTaskId.value = null
}

async function handleRetry(taskId: string, e: Event) {
  e.stopPropagation()
  try {
    await taskStore.retryTask(taskId)
  } catch (err) {
    console.error('Failed to retry task:', err)
  }
}

function handleReuse(task: TaskSummary, e: Event) {
  e.stopPropagation()
  router.push({
    path: '/synthesize',
    query: {
      text: task.custom_title || task.title || '',
      voice: task.voice || '',
    },
  })
}

async function handlePlay(task: TaskSummary, e: Event) {
  e.stopPropagation()
  const audioUrl = api.getAudioUrl(task.id)
  // If this task is already playing, stop it
  if (audioStore.currentUrl === audioUrl && audioStore.isPlaying) {
    audioStore.stop()
    return
  }
  playingTaskId.value = task.id
  try {
    await taskStore.getTaskDetail(task.id)
    audioStore.play(audioUrl)
  } catch (err) {
    console.error('Failed to play audio:', err)
  } finally {
    playingTaskId.value = null
  }
}

function handleClearAll() {
  confirmClearDialogOpen.value = true
}

async function confirmClearAll() {
  isClearingAll.value = true
  try {
    await taskStore.clearAll()
    rowSelection.value = {}
    toast.success('全部任务已清空')
  } catch (err: any) {
    console.error('Failed to clear all tasks:', err)
    toast.error('清空失败: ' + (err.message || '未知错误'))
  }
  isClearingAll.value = false
  confirmClearDialogOpen.value = false
}

async function handleBatchDelete() {
  isBatchDeleting.value = true
  const ids = selectedTaskIds.value
  let failed = 0
  await Promise.all(
    ids.map(async (id) => {
      try {
        await taskStore.removeTask(id)
      } catch {
        failed++
      }
    }),
  )
  rowSelection.value = {}
  isBatchDeleting.value = false
  if (failed === 0) {
    toast.success(`已删除 ${ids.length} 个任务`)
  } else {
    toast.error(`删除完成，${failed}/${ids.length} 个失败`)
  }
}

// ── Pagination ────────────────────────────────────────────────
async function goToPage(page: number) {
  if (page < 0 || page >= totalPages.value) return
  localPageIndex.value = page
  await taskStore.loadPage(page)
}

async function goToNextPage() {
  if (!isLastPage.value) await goToPage(localPageIndex.value + 1)
}

async function goToPrevPage() {
  if (!isFirstPage.value) await goToPage(localPageIndex.value - 1)
}

async function goToFirstPage() {
  await goToPage(0)
}

async function goToLastPage() {
  await goToPage(totalPages.value - 1)
}

// ── Column definitions ────────────────────────────────────────
const columns: ColumnDef<TaskSummary, unknown>[] = [
  {
    id: 'select',
    header: ({ table }) =>
      h('div', { class: 'flex items-center' }, [
        h('input', {
          type: 'checkbox',
          class: 'rounded border-input size-4 cursor-pointer accent-primary',
          checked: table.getIsAllPageRowsSelected(),
          indeterminate: table.getIsSomePageRowsSelected(),
          onChange: (e: Event) => {
            table.toggleAllPageRowsSelected((e.target as HTMLInputElement).checked)
          },
        }),
      ]),
    cell: ({ row }) =>
      h('div', { class: 'flex items-center' }, [
        h('input', {
          type: 'checkbox',
          class: 'rounded border-input size-4 cursor-pointer accent-primary',
          checked: row.getIsSelected(),
          onChange: (e: Event) => {
            row.toggleSelected((e.target as HTMLInputElement).checked)
          },
        }),
      ]),
    enableSorting: false,
    enableHiding: false,
    size: 40,
  },
  {
    id: 'name',
    header: '任务',
    cell: ({ row }) => {
      const task = row.original
      const hasName = !!(task.custom_title || task.title)
      const children: ReturnType<typeof h>[] = []
      if (hasName) {
        children.push(h('span', { class: 'text-sm font-medium truncate' }, task.custom_title || task.title!))
        children.push(h('span', { class: 'font-mono text-xs text-muted-foreground/60 truncate' }, task.id))
      } else {
        children.push(h('span', { class: 'text-xs font-mono text-muted-foreground truncate' }, task.id))
      }
      return h('div', { class: 'flex flex-col gap-0.5 min-w-0 cursor-pointer', onClick: () => navigateToTask(task.id) }, children)
    },
    size: 240,
    enableSorting: false,
  },
  {
    accessorKey: 'status',
    header: '状态',
    cell: ({ row }) => {
      const status = row.getValue('status') as TaskStatus
      return h(
        Badge,
        { variant: getTaskStatusVariant(status), class: 'text-xs' },
        () => getTaskStatusText(status),
      )
    },
    size: 100,
    enableSorting: false,
  },
  {
    accessorKey: 'voice',
    header: '音色',
    cell: ({ row }) => {
      const voice = row.getValue('voice') as string | null
      const info = getVoiceDisplay(voice)
      if (!info) return h('span', { class: 'text-xs text-muted-foreground' }, '—')
      const isFemale = info.gender === '女性' || info.gender === 'Female'
      const colorClass = isFemale
        ? 'text-rose-600 dark:text-rose-400 bg-rose-50 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-800/40'
        : 'text-sky-600 dark:text-sky-400 bg-sky-50 dark:bg-sky-950/30 border border-sky-200 dark:border-sky-800/40'
      return h('span', { class: `inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium ${colorClass}` }, info.name)
    },
    size: 100,
    enableSorting: false,
  },
  {
    id: 'provider',
    header: '服务商',
    cell: ({ row }) => {
      const name = (row.original as any)._providerName as string
      if (!name) return h('span', { class: 'text-xs text-muted-foreground' }, '—')
      return h('span', { class: 'text-xs text-muted-foreground' }, name)
    },
    size: 100,
    enableSorting: false,
  },
  {
    id: 'tokenDisplay',
    header: 'Tokens',
    cell: ({ row }) => {
      const displayTokens = (row.original as any)._displayTokens as number
      if (displayTokens === 0) return h('span', { class: 'text-xs text-muted-foreground/50' }, '—')
      return h('span', { class: 'text-xs tabular-nums text-muted-foreground' }, formatTokens(displayTokens))
    },
    size: 80,
    enableSorting: false,
  },
  {
    accessorKey: 'created_at',
    header: ({ column }) => {
      const isSorted = column.getIsSorted()
      return h('div', {
        class: 'flex items-center gap-1 cursor-pointer select-none',
        onClick: column.getToggleSortingHandler(),
      }, [
        h('span', {}, '创建时间'),
        isSorted === 'asc'
          ? h(ArrowUp, { class: 'w-3 h-3 shrink-0' })
          : isSorted === 'desc'
            ? h(ArrowDown, { class: 'w-3 h-3 shrink-0' })
            : h(ArrowUpDown, { class: 'w-3 h-3 shrink-0 opacity-40' }),
      ])
    },
    cell: ({ row }) =>
      h('span', { class: 'text-xs text-muted-foreground tabular-nums' }, formatLocalDateTime(row.getValue('created_at') as string)),
    size: 160,
    enableSorting: true,
  },
  {
    accessorKey: 'completed_at',
    header: '完成时间',
    cell: ({ row }) => {
      const val = row.getValue('completed_at') as string | null
      if (!val) return h('span', { class: 'text-xs text-muted-foreground/50' }, '—')
      return h('span', { class: 'text-xs text-muted-foreground tabular-nums' }, formatLocalDateTime(val))
    },
    size: 160,
    enableSorting: false,
  },
  {
    id: 'actions',
    header: '操作',
    cell: ({ row }) => {
      const task = row.original
      const buttons: ReturnType<typeof h>[] = []

      if (task.has_audio) {
        const isThisPlaying = currentlyPlayingId.value === task.id
        const isLoadingThis = playingTaskId.value === task.id
        buttons.push(
          h(
            Button,
            {
              variant: 'ghost',
              size: 'sm',
              class: `h-7 w-7 p-0 ${isThisPlaying ? 'text-primary' : ''}`,
              title: isThisPlaying ? '停止' : '播放',
              onClick: (e: Event) => handlePlay(task, e),
            },
            () => isLoadingThis
              ? h(Loader2, { class: 'w-3.5 h-3.5 animate-spin' })
              : isThisPlaying
                ? h(Square, { class: 'w-3.5 h-3.5' })
                : h(Play, { class: 'w-3.5 h-3.5' }),
          ),
        )
        buttons.push(
          h(
            Button,
            {
              variant: 'ghost',
              size: 'sm',
              class: 'h-7 w-7 p-0',
              title: '下载',
              onClick: (e: Event) => handleDownload(task.id, e),
            },
            () => h(Download, { class: 'w-3.5 h-3.5' }),
          ),
        )
      }

      buttons.push(
        h(
          Button,
          {
            variant: 'ghost',
            size: 'sm',
            class: 'h-7 w-7 p-0',
            title: '复用',
            onClick: (e: Event) => handleReuse(task, e),
          },
          () => h(Copy, { class: 'w-3.5 h-3.5' }),
        ),
      )

      if (task.status === 'failed') {
        buttons.push(
          h(
            Button,
            {
              variant: 'ghost',
              size: 'sm',
              class: 'h-7 w-7 p-0',
              title: '重试',
              onClick: (e: Event) => handleRetry(task.id, e),
            },
            () => h(RotateCcw, { class: 'w-3.5 h-3.5' }),
          ),
        )
      }

      buttons.push(
        h(
          Button,
          {
            variant: 'ghost',
            size: 'sm',
            class: 'h-7 w-7 p-0 text-destructive hover:text-destructive',
            title: '删除',
            onClick: (e: Event) => handleDelete(task.id, e),
          },
          () => h(Trash2, { class: 'w-3.5 h-3.5' }),
        ),
      )

      return h('div', { class: 'flex items-center gap-0.5' }, buttons)
    },
    size: 190,
    enableSorting: false,
  },
]

// ── Table instance ────────────────────────────────────────────
const table = useVueTable({
  get data() {
    return enrichedTasks.value
  },
  columns,
  getCoreRowModel: getCoreRowModel(),
  getSortedRowModel: getSortedRowModel(),
  manualPagination: true,
  pageCount: totalPages.value,
  enableRowSelection: true,
  state: {
    get pagination() {
      return {
        pageIndex: localPageIndex.value,
        pageSize: localPageSize.value,
      }
    },
    get rowSelection() {
      return rowSelection.value
    },
    get sorting() {
      return sorting.value
    },
  },
  onRowSelectionChange: (updater) => {
    rowSelection.value = typeof updater === 'function' ? updater(rowSelection.value) : updater
  },
  onSortingChange: (updater) => {
    sorting.value = typeof updater === 'function' ? updater(sorting.value) : updater
  },
  getRowId: (row) => row.id,
})

// ── Lifecycle ─────────────────────────────────────────────────
onMounted(() => {
})

// ── Refresh ───────────────────────────────────────────────────
async function refresh() {
  localPageIndex.value = 0
  await taskStore.loadTasks()
}
</script>

<template>
  <div class="h-full flex flex-col w-full max-w-[1600px] mx-auto">
    <Card class="h-full flex flex-col overflow-hidden">
      <!-- ═══ Toolbar ═══ -->
      <div class="flex items-center gap-3 px-4 pt-4 pb-3 shrink-0 flex-wrap">
        <!-- Search -->
        <div class="relative flex-1 min-w-[200px]">
          <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground pointer-events-none" />
          <Input
            v-model="searchQuery"
            placeholder="搜索任务名称或ID..."
            class="pl-9 h-8 text-sm"
          />
        </div>

        <!-- Status filter -->
        <Select :model-value="statusFilterValue" @update:model-value="onStatusFilterChange">
          <SelectTrigger class="w-[130px] h-8 text-sm">
            <SelectValue placeholder="全部状态" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem
              v-for="opt in statusOptions"
              :key="opt.value"
              :value="opt.value"
            >
              {{ opt.label }}
            </SelectItem>
          </SelectContent>
        </Select>

        <!-- Voice filter -->
        <Select v-model="voiceFilter">
          <SelectTrigger class="w-[130px] h-8 text-sm">
            <SelectValue placeholder="全部音色" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">全部音色</SelectItem>
            <SelectItem
              v-for="v in availableVoices"
              :key="v.id"
              :value="v.id"
            >
              {{ v.name }}
            </SelectItem>
          </SelectContent>
        </Select>

        <!-- Provider filter -->
        <Select v-model="providerFilter">
          <SelectTrigger class="w-[130px] h-8 text-sm">
            <SelectValue placeholder="全部服务商" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">全部服务商</SelectItem>
            <SelectItem
              v-for="p in availableProviders"
              :key="p.id"
              :value="p.id"
            >
              {{ p.name }}
            </SelectItem>
          </SelectContent>
        </Select>

        <!-- Batch delete (visible when rows selected) -->
        <Button
          v-if="selectedIdsCount > 0"
          variant="destructive"
          size="sm"
          class="h-8 px-2.5 shrink-0 gap-1.5"
          :disabled="isBatchDeleting"
          @click="handleBatchDelete"
        >
          <Loader2 v-if="isBatchDeleting" class="w-3.5 h-3.5 animate-spin" />
          <Trash2 v-else class="w-3.5 h-3.5" />
          <span>{{ isBatchDeleting ? '删除中...' : '批量删除' }}</span>
          <span v-if="!isBatchDeleting" class="inline-flex items-center justify-center rounded-full bg-primary-foreground/20 px-1.5 text-[10px] font-medium tabular-nums leading-none">
            {{ selectedIdsCount }}
          </span>
        </Button>

        <!-- Clear all -->
        <Button
          variant="outline"
          size="sm"
          class="h-8 px-2.5 shrink-0 gap-1.5"
          @click="handleClearAll"
        >
          <Eraser class="w-3.5 h-3.5" />
          <span>清空全部</span>
        </Button>

        <!-- Refresh -->
        <Button
          variant="outline"
          size="sm"
          class="h-8 px-2.5 shrink-0"
          :disabled="isLoading"
          @click="refresh"
        >
          <Loader2 v-if="isLoading" class="w-4 h-4 animate-spin" />
          <span v-else>刷新</span>
        </Button>
      </div>

      <!-- ═══ Table content ═══ -->
      <CardContent class="flex-1 overflow-hidden p-0 px-4 pb-2 min-h-0">
        <!-- Loading skeleton -->
        <div v-if="isInitialLoad" class="space-y-2 pt-2">
          <Skeleton v-for="n in 8" :key="n" class="h-11 w-full rounded-md" />
        </div>

        <!-- Empty state -->
        <div v-else-if="isEmpty" class="flex flex-col items-center justify-center py-16 text-muted-foreground">
          <Inbox class="w-12 h-12 mb-4 opacity-40" />
          <p class="text-sm">暂无任务</p>
        </div>

        <!-- Table -->
        <div v-else ref="scrollContainerRef" class="overflow-auto h-full">
          <Table class="table-fixed">
            <TableHeader class="sticky top-0 z-10 bg-background">
              <TableRow
                v-for="headerGroup in table.getHeaderGroups()"
                :key="headerGroup.id"
              >
                <TableHead
                  v-for="header in headerGroup.headers"
                  :key="header.id"
                  :style="header.column.columnDef.size ? { width: `${header.column.columnDef.size}px` } : undefined"
                >
                  <template v-if="header.isPlaceholder">&nbsp;</template>
                  <FlexRender
                    v-else
                    :render="header.column.columnDef.header"
                    :props="header.getContext()"
                  />
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow
                v-for="row in table.getRowModel().rows"
                :key="row.id"
                :data-state="row.getIsSelected() ? 'selected' : undefined"
                class="cursor-pointer"
                @click="navigateToTask(row.original.id)"
              >
                <TableCell
                  v-for="cell in row.getVisibleCells()"
                  :key="cell.id"
                  :style="cell.column.columnDef.size ? { width: `${cell.column.columnDef.size}px` } : undefined"
                >
                  <FlexRender
                    :render="cell.column.columnDef.cell"
                    :props="cell.getContext()"
                  />
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </div>
      </CardContent>

      <!-- ═══ Pagination footer ═══ -->
      <div
        v-if="tasks.length > 0 || taskStore.totalCount > 0"
        class="flex items-center justify-between gap-4 px-4 py-2 border-t shrink-0 flex-wrap"
      >
        <!-- Left: page info + per page -->
        <div class="flex items-center gap-3 text-xs text-muted-foreground">
          <span>{{ pageInfo }}</span>
          <span class="text-border">|</span>
          <Select
            :model-value="String(localPageSize)"
            @update:model-value="(v) => {
              localPageSize = Number(v)
              localPageIndex = 0
              taskStore.perPage = Number(v)
              taskStore.loadPage(0)
            }"
          >
            <SelectTrigger class="h-7 w-[80px] text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="20">20 条</SelectItem>
              <SelectItem value="50">50 条</SelectItem>
              <SelectItem value="100">100 条</SelectItem>
              <SelectItem value="200">200 条</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <!-- Right: page navigation -->
        <div class="flex items-center gap-1">
          <Button
            variant="outline"
            size="sm"
            class="h-7 w-7 p-0"
            :disabled="isFirstPage"
            @click="goToFirstPage"
          >
            <ChevronsLeft class="w-4 h-4" />
          </Button>
          <Button
            variant="outline"
            size="sm"
            class="h-7 w-7 p-0"
            :disabled="isFirstPage"
            @click="goToPrevPage"
          >
            <ChevronLeft class="w-4 h-4" />
          </Button>
          <span class="text-xs text-muted-foreground px-2 tabular-nums min-w-[60px] text-center">
            {{ localPageIndex + 1 }} / {{ totalPages || 1 }}
          </span>
          <Button
            variant="outline"
            size="sm"
            class="h-7 w-7 p-0"
            :disabled="isLastPage"
            @click="goToNextPage"
          >
            <ChevronRight class="w-4 h-4" />
          </Button>
          <Button
            variant="outline"
            size="sm"
            class="h-7 w-7 p-0"
            :disabled="isLastPage"
            @click="goToLastPage"
          >
            <ChevronsRight class="w-4 h-4" />
          </Button>
        </div>
      </div>
    </Card>

    <!-- ═══ Confirm delete dialog ═══ -->
    <Dialog :open="confirmDeleteTaskId !== null" @update:open="(v: boolean) => { if (!v) confirmDeleteTaskId = null }">
      <DialogContent class="sm:max-w-[400px]">
        <DialogHeader>
          <DialogTitle class="flex items-center gap-2">
            <AlertTriangle class="w-5 h-5 text-destructive" />
            确认删除
          </DialogTitle>
          <DialogDescription>
            确定要删除此任务吗？此操作不可撤销。
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose as-child>
            <Button variant="outline" size="sm">取消</Button>
          </DialogClose>
          <Button variant="destructive" size="sm" @click="confirmDelete">删除</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>

    <!-- ═══ Confirm clear all dialog ═══ -->
    <Dialog v-model:open="confirmClearDialogOpen">
      <DialogContent class="sm:max-w-[400px]">
        <DialogHeader>
          <DialogTitle class="flex items-center gap-2">
            <AlertTriangle class="w-5 h-5 text-destructive" />
            确认清空全部任务
          </DialogTitle>
          <DialogDescription>
            确定要清空全部任务吗？此操作不可撤销。
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose as-child>
            <Button variant="outline" size="sm" :disabled="isClearingAll">取消</Button>
          </DialogClose>
          <Button variant="destructive" size="sm" :disabled="isClearingAll" @click="confirmClearAll">
            <Loader2 v-if="isClearingAll" class="w-3.5 h-3.5 mr-1.5 animate-spin" />
            {{ isClearingAll ? '清空中...' : '确认清空' }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
