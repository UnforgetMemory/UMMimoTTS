import { defineStore } from 'pinia'
import { ref, computed, shallowRef, type ShallowRef } from 'vue'
import { api, type Task, type TaskSummary, type TaskStatus, type TaskEvent, type TaskListParams } from '@/api/client'

export const useTaskStore = defineStore('task', () => {
  // ── Task Map (shallowRef for large-scale performance) ──────────
  const taskMap: ShallowRef<Map<string, TaskSummary>> = shallowRef(new Map())

  const loading = ref(false)
  const refreshing = ref(false)
  const error = ref<string | null>(null)

  // ── Pagination state ──────────────────────────────────────────
  const currentPage = ref(0)
  const perPage = ref(50)
  const totalCount = ref(0)
  const hasMore = computed(() => {
    const totalPages = Math.ceil(totalCount.value / perPage.value)
    return currentPage.value < totalPages - 1
  })

  // ── Search / Filter state ─────────────────────────────────────
  const activeSearch = ref('')
  const activeStatus = ref<TaskStatus | undefined>(undefined)
  const activeGroupFilter = ref<string | undefined>(undefined)

  // ── Detail cache (non-reactive) ───────────────────────────────
  const taskDetailCache = new Map<string, Task>()
  const detailLoading = ref(false)

  // ── Computed: all tasks as array ──────────────────────────────
  const allTasks = computed(() => Array.from(taskMap.value.values()))

  /** Tasks not belonging to any batch group */
  const standaloneTasks = computed(() =>
    allTasks.value.filter(t => !t.group_id)
  )

  const completedTasks = computed(() =>
    allTasks.value.filter(t => t.status === 'completed')
  )

  const failedTasks = computed(() =>
    allTasks.value.filter(t => t.status === 'failed')
  )

  const pendingTasks = computed(() =>
    allTasks.value.filter(t =>
      ['pending', 'queued', 'synthesizing', 'streaming'].includes(t.status)
    )
  )

  // ── Paginated loading ────────────────────────────────────────

  /**
   * Load a specific page from the paginated API and merge into taskMap.
   * Resets accumulated tasks if page === 0.
   */
  async function loadPage(page = 0) {
    const params: TaskListParams = {
      page,
      per_page: perPage.value,
    }
    if (activeSearch.value) params.search = activeSearch.value
    if (activeStatus.value) params.status = activeStatus.value
    if (activeGroupFilter.value) params.group_id = activeGroupFilter.value

    try {
      const result = await api.getTasksPaginated(params)

      const newMap = page === 0 ? new Map<string, TaskSummary>() : new Map(taskMap.value)

      for (const item of result.items) {
        newMap.set(item.id, item)
      }

      taskMap.value = newMap
      currentPage.value = page
      totalCount.value = result.total
    } catch (err: any) {
      error.value = err.message || '加载任务失败'
      console.error('Failed to load tasks page:', err)
    }
  }

  /**
   * Load the next page if more results are available.
   */
  async function loadMore() {
    if (!hasMore.value || loading.value) return
    loading.value = true
    try {
      await loadPage(currentPage.value + 1)
    } finally {
      loading.value = false
    }
  }

  /**
   * Initial load of the first page.
   */
  async function loadTasks() {
    const MIN_DURATION = 300
    const start = Date.now()
    refreshing.value = true
    error.value = null
    try {
      await loadPage(0)
    } catch (err: any) {
      error.value = err.message || '加载任务失败'
      console.error('Failed to load tasks:', err)
    } finally {
      const elapsed = Date.now() - start
      if (elapsed < MIN_DURATION) {
        await new Promise(r => setTimeout(r, MIN_DURATION - elapsed))
      }
      refreshing.value = false
    }
  }

  // ── Detail fetching ───────────────────────────────────────────

  /**
   * Get detailed Task (with text/context/model) by ID.
   * Uses a non-reactive cache to avoid memory bloat.
   */
  async function getTaskDetail(id: string): Promise<Task> {
    // Check detail cache first
    const cached = taskDetailCache.get(id)
    if (cached) return cached

    detailLoading.value = true
    try {
      const task = await api.getTask(id)
      taskDetailCache.set(id, task)
      return task
    } finally {
      detailLoading.value = false
    }
  }

  // ── Search / Filter ───────────────────────────────────────────

  async function searchTasks(query: string) {
    activeSearch.value = query
    activeStatus.value = undefined
    await loadPage(0)
  }

  async function filterByStatus(status: TaskStatus | undefined) {
    activeStatus.value = status
    await loadPage(0)
  }

  async function filterByGroup(groupId: string | undefined) {
    activeGroupFilter.value = groupId
    await loadPage(0)
  }

  // ── Update single task in map (for SSE) ───────────────────────

  function updateTaskInMap(taskId: string, updates: Partial<TaskSummary>) {
    const existing = taskMap.value.get(taskId)
    if (existing) {
      const newMap = new Map(taskMap.value)
      newMap.set(taskId, { ...existing, ...updates })
      taskMap.value = newMap
    }
  }

  // ── CRUD ──────────────────────────────────────────────────────

  async function createTask(request: {
    text: string
    voice: string
    model: string
    context?: string
    task_name?: string
    api_key?: string
  }) {
    loading.value = true
    error.value = null
    try {
      const result = await api.synthesize(request)
      // Use lightweight page reload instead of full loadTasks
      await loadPage(0)

      // 订阅该任务的 SSE 事件
      subscribeToTaskEvents(result.task_id)

      return result.task_id
    } catch (err: any) {
      error.value = err.response?.data?.message || err.message || '创建任务失败'
      throw err
    } finally {
      loading.value = false
    }
  }

  async function removeTask(taskId: string) {
    try {
      await api.deleteTask(taskId)
      const newMap = new Map(taskMap.value)
      newMap.delete(taskId)
      taskMap.value = newMap
      taskDetailCache.delete(taskId)
    } catch (err: any) {
      error.value = err.message || '删除任务失败'
      console.error('Failed to delete task:', err)
      throw err
    }
  }

  // ── SSE subscription ──────────────────────────────────────────

  const eventSources = new Map<string, EventSource>()

  function subscribeToTaskEvents(taskId: string) {
    // 如果已经订阅过，先关闭旧的连接
    if (eventSources.has(taskId)) {
      eventSources.get(taskId)?.close()
    }

    const eventSource = api.subscribeToTask(taskId, (event: TaskEvent) => {
      console.log('SSE Event received:', event)

      switch (event.event_type) {
        case 'status_changed':
          updateTaskInMap(taskId, {
            status: event.status,
            progress: event.progress ?? 0,
          })
          break
        case 'completed':
          updateTaskInMap(taskId, {
            status: 'completed',
            progress: 1.0,
          })
          // 完成后关闭连接
          eventSources.delete(taskId)
          eventSource.close()
          // Lightweight page reload instead of full loadTasks
          loadPage(0)
          break
        case 'failed':
          updateTaskInMap(taskId, {
            status: 'failed',
          })
          // 失败后关闭连接
          eventSources.delete(taskId)
          eventSource.close()
          loadPage(0)
          break
      }
    })

    eventSources.set(taskId, eventSource)
  }

  // ── Polling fallback ──────────────────────────────────────────

  let pollTimer: ReturnType<typeof setInterval> | null = null

  function startPolling() {
    stopPolling()
    pollTimer = setInterval(() => {
      loadTasks()
    }, 30000)
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  }

  // ── Reset ────────────────────────────────────────────────────

  /** Clears all state — useful on logout or full refresh */
  function resetStore() {
    taskMap.value = new Map()
    taskDetailCache.clear()
    currentPage.value = 0
    totalCount.value = 0
    activeSearch.value = ''
    activeStatus.value = undefined
    activeGroupFilter.value = undefined
    error.value = null
    loading.value = false
    refreshing.value = false
  }

  // ── Cleanup ───────────────────────────────────────────────────

  function cleanup() {
    eventSources.forEach(es => es.close())
    eventSources.clear()
    stopPolling()
    taskDetailCache.clear()
  }

  // ── Init ──────────────────────────────────────────────────────

  function init() {
    const MIN_DURATION = 300
    const start = Date.now()
    loading.value = true
    loadTasks().finally(() => {
      const elapsed = Date.now() - start
      if (elapsed < MIN_DURATION) {
        setTimeout(() => { loading.value = false }, MIN_DURATION - elapsed)
      } else {
        loading.value = false
      }
    })
    startPolling()
  }

  return {
    // State
    taskMap,
    allTasks,
    loading,
    refreshing,
    error,
    currentPage,
    perPage,
    totalCount,
    hasMore,
    activeSearch,

    // Computed
    completedTasks,
    failedTasks,
    pendingTasks,
    standaloneTasks,

    // Paginated load
    loadTasks,
    loadPage,
    loadMore,

    // Detail
    getTaskDetail,
    detailLoading,
    taskDetailCache,

    // Search / Filter
    searchTasks,
    filterByStatus,
    filterByGroup,

    // CRUD
    createTask,
    removeTask,

    // SSE
    updateTaskInMap,
    init,
    cleanup,
    resetStore,
    startPolling,
    stopPolling,
  }
})
