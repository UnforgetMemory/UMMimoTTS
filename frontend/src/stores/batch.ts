import { defineStore } from 'pinia'
import { ref, computed, shallowRef, type ShallowRef } from 'vue'
import {
  api,
  type GroupSummary,
  type BatchGroup,
  type TaskSummary,
  type GroupUpdateRequest,
  type GroupEvent,
  type GroupStatus,
  type TaskConfig,
} from '@/api/client'

export const useBatchStore = defineStore('batch', () => {
  // ── Group Map (shallowRef for large-scale performance) ──────────
  const groupMap: ShallowRef<Map<string, GroupSummary>> = shallowRef(new Map())

  const loading = ref(false)
  const refreshing = ref(false)
  const error = ref<string | null>(null)
  /** 服务端批量处理上限，启动时拉取 */
  const batchLimit = ref<number>(100)
  /** 正在下载的分组ID */
  const downloadingGroupId = ref<string | null>(null)

  // ── Pagination state ──────────────────────────────────────────
  const currentPage = ref(0)
  const perPage = ref(20)
  const totalCount = ref(0)
  const hasMore = computed(() => {
    const totalPages = Math.ceil(totalCount.value / perPage.value)
    return currentPage.value < totalPages - 1
  })

  // ── Per-group task cache ───────────────────────────────────────
  const groupTaskCache = new Map<string, {
    tasks: TaskSummary[]
    loaded: boolean
    hasMore: boolean
    page: number
  }>()

  // ── SSE 连接管理 ──────────────────────────────────────────────
  const eventSources = new Map<string, EventSource>()

  // ── 计算属性 ───────────────────────────────────────────────────

  /** All groups as sorted array (newest first) */
  const allGroups = computed<GroupSummary[]>(() =>
    Array.from(groupMap.value.values()).sort(
      (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
    ),
  )

  /** 正在处理中的分组 */
  const activeGroups = computed(() =>
    allGroups.value.filter((g) => g.status === 'processing'),
  )

  /** 已完成的分组 */
  const completedGroups = computed(() =>
    allGroups.value.filter((g) => g.status === 'completed'),
  )

  /** 失败的分组 */
  const failedGroups = computed(() =>
    allGroups.value.filter((g) => g.status === 'failed'),
  )

  // ── 内部工具 ───────────────────────────────────────────────────

  /** Replace groupMap contents (triggers shallowRef reactivity) */
  function setGroupMap(items: GroupSummary[]) {
    const newMap = new Map<string, GroupSummary>()
    for (const item of items) {
      newMap.set(item.id, item)
    }
    groupMap.value = newMap
  }

  /** Merge groups into the map (append/update existing) */
  function mergeGroups(items: GroupSummary[]) {
    const newMap = new Map(groupMap.value)
    for (const item of items) {
      newMap.set(item.id, item)
    }
    groupMap.value = newMap
  }

  /** Update a single group's fields in the map */
  function updateGroupInMap(groupId: string, updates: Partial<GroupSummary>) {
    const existing = groupMap.value.get(groupId)
    if (existing) {
      const newMap = new Map(groupMap.value)
      newMap.set(groupId, { ...existing, ...updates })
      groupMap.value = newMap
    }
  }

  // ── API 方法 ──────────────────────────────────────────────────

  /**
   * Load a specific page of groups and merge into groupMap.
   */
  async function loadPage(page = 0) {
    try {
      const result = await api.getGroupsPaginated(page, perPage.value)
      if (page === 0) {
        setGroupMap(result.items as GroupSummary[])
      } else {
        mergeGroups(result.items as GroupSummary[])
      }
      currentPage.value = page
      totalCount.value = result.total
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : '加载分组失败'
      error.value = message
      console.error('Failed to load groups:', err)
    }
  }

  /**
   * Load next page of groups
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
  async function loadGroups() {
    const MIN_DURATION = 300
    const start = Date.now()
    refreshing.value = true
    error.value = null
    try {
      await loadPage(0)
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : '加载分组失败'
      error.value = message
      console.error('Failed to load groups:', err)
    } finally {
      const elapsed = Date.now() - start
      if (elapsed < MIN_DURATION) {
        await new Promise((r) => setTimeout(r, MIN_DURATION - elapsed))
      }
      refreshing.value = false
    }
  }

  /** Legacy alias for loadGroups */
  const loadGroupsPage = loadPage

  /** Legacy alias for loadMore */
  const loadMoreGroups = loadMore

  /**
   * Load group detail with paginated tasks (lazy).
   * Calls the combined endpoint and stores tasks in the group task cache.
   */
  async function getGroupDetailWithTasks(groupId: string, page = 0, tasksPerPage = 50) {
    try {
      const result = await api.getGroupDetailWithTasks(groupId, page, tasksPerPage)

      // Update the group in the map
      updateGroupInMap(groupId, result.group)

      // Store tasks in the per-group cache
      const existing = groupTaskCache.get(groupId)
      const tasks: TaskSummary[] = page === 0 || !existing
        ? result.tasks.items
        : [...existing.tasks, ...result.tasks.items]

      groupTaskCache.set(groupId, {
        tasks,
        loaded: page === 0 || (result.tasks.page >= result.tasks.total_pages - 1),
        hasMore: result.tasks.page < result.tasks.total_pages - 1,
        page: result.tasks.page - 1, // store as 0-based internally
      })

      return result
    } catch (err) {
      console.error(`Failed to load group detail for ${groupId}:`, err)
      throw err
    }
  }

  /**
   * Lazy load tasks for a specific group (paginated, legacy path)
   */
  async function loadGroupTasks(groupId: string, page = 0, force = false) {
    const cache = groupTaskCache.get(groupId)
    if (cache && cache.loaded && !force && page === 0) return

    try {
      const result = await api.getGroupTasks(groupId, page, 50)

      const existing = groupTaskCache.get(groupId)
      const tasks: TaskSummary[] = page === 0 || !existing
        ? result.items
        : [...existing.tasks, ...result.items]

      groupTaskCache.set(groupId, {
        tasks,
        loaded: page === 0 || (result.page >= result.total_pages - 1),
        hasMore: result.page < result.total_pages - 1,
        page: result.page - 1, // store as 0-based internally
      })
    } catch (err) {
      console.error(`Failed to load tasks for group ${groupId}:`, err)
    }
  }

  /**
   * Get cached tasks for a group (or empty array if not loaded)
   */
  function getGroupTasks(groupId: string): TaskSummary[] {
    return groupTaskCache.get(groupId)?.tasks ?? []
  }

  /**
   * Load more tasks for a group (next page)
   */
  async function loadMoreGroupTasks(groupId: string) {
    const cache = groupTaskCache.get(groupId)
    if (!cache || !cache.hasMore) return
    await loadGroupTasks(groupId, cache.page + 1)
  }

  /**
   * 批量导入文件并创建分组
   */
  async function importBatch(
    files: File[],
    options?: {
      name?: string
      voice?: string
      model?: string
      context?: string
      taskConfigs?: TaskConfig[]
      useFilenameAsTaskName?: boolean
      apiKey?: string
    },
  ) {
    loading.value = true
    error.value = null
    try {
      const result = await api.importBatch({
        files,
        group_name: options?.name,
        voice: options?.voice,
        model: options?.model,
        context: options?.context,
        task_configs: options?.taskConfigs,
        use_filename_as_task_name: options?.useFilenameAsTaskName,
        api_key: options?.apiKey,
      })
      await loadGroups()

      // 订阅该分组的 SSE 事件
      subscribeToGroupEvents(result.group_id)

      return result.group_id
    } catch (err: unknown) {
      const message =
        err instanceof Error
          ? (err as any).response?.data?.message || err.message
          : '批量导入失败'
      error.value = message
      throw err
    } finally {
      loading.value = false
    }
  }

  /**
   * 更新分组设置
   */
  async function updateGroup(groupId: string, request: GroupUpdateRequest) {
    error.value = null
    try {
      const updated: BatchGroup = await api.updateGroup(groupId, request)
      // Update local from response
      updateGroupInMap(groupId, {
        name: updated.name,
        voice: updated.voice,
        model: updated.model,
        context: updated.context,
      })
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : '更新分组失败'
      error.value = message
      console.error('Failed to update group:', err)
      throw err
    }
  }

  /**
   * 暂停分组处理
   */
  async function pauseGroup(groupId: string) {
    error.value = null
    try {
      await api.pauseGroup(groupId)
      updateGroupInMap(groupId, { status: 'paused' as GroupStatus })
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : '暂停分组失败'
      error.value = message
      console.error('Failed to pause group:', err)
      throw err
    }
  }

  /**
   * 恢复分组处理
   */
  async function resumeGroup(groupId: string) {
    error.value = null
    try {
      await api.resumeGroup(groupId)
      updateGroupInMap(groupId, { status: 'processing' as GroupStatus })
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : '恢复分组失败'
      error.value = message
      console.error('Failed to resume group:', err)
      throw err
    }
  }

  /**
   * 重试分组中失败的任务
   */
  async function retryFailed(groupId: string) {
    error.value = null
    try {
      await api.retryFailed(groupId)
      updateGroupInMap(groupId, { status: 'processing' as GroupStatus, failed_tasks: 0 })
      // 重新订阅 SSE 事件
      subscribeToGroupEvents(groupId)
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : '重试失败任务失败'
      error.value = message
      console.error('Failed to retry failed tasks:', err)
      throw err
    }
  }

  /**
   * 删除分组及其关联任务
   */
  async function removeGroup(groupId: string) {
    error.value = null
    try {
      await api.deleteGroup(groupId)
      const newMap = new Map(groupMap.value)
      newMap.delete(groupId)
      groupMap.value = newMap
      // 清理 SSE 连接
      const es = eventSources.get(groupId)
      if (es) {
        es.close()
        eventSources.delete(groupId)
      }
      groupTaskCache.delete(groupId)
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : '删除分组失败'
      error.value = message
      console.error('Failed to delete group:', err)
      throw err
    }
  }

  /**
   * 订阅分组 SSE 事件，实时更新分组状态
   */
  function subscribeToGroupEvents(groupId: string) {
    // 如果已订阅，先关闭旧连接
    if (eventSources.has(groupId)) {
      eventSources.get(groupId)?.close()
    }

    const eventSource = api.subscribeToGroup(groupId, (event: GroupEvent) => {
      console.log('Group SSE event:', event)

      switch (event.event_type) {
        case 'progress':
          updateGroupInMap(groupId, {
            completed_tasks: event.completed_tasks ?? 0,
            failed_tasks: event.failed_tasks ?? 0,
            total_tasks: event.total_tasks ?? 0,
            ...(event.group_status && { status: event.group_status as GroupStatus }),
          })
          break
        case 'task_completed':
          updateGroupInMap(groupId, {
            completed_tasks: event.completed_tasks ?? 0,
            total_tasks: event.total_tasks ?? 0,
            ...(event.group_status && { status: event.group_status as GroupStatus }),
          })
          break
        case 'task_failed':
          updateGroupInMap(groupId, {
            failed_tasks: event.failed_tasks ?? 0,
            total_tasks: event.total_tasks ?? 0,
            ...(event.group_status && { status: event.group_status as GroupStatus }),
          })
          break
        case 'group_completed':
          updateGroupInMap(groupId, {
            status: 'completed' as GroupStatus,
            completed_tasks: event.completed_tasks ?? 0,
          })
          // 完成后关闭连接并刷新
          eventSources.delete(groupId)
          eventSource.close()
          loadGroups()
          break
        case 'group_failed':
          updateGroupInMap(groupId, {
            status: 'failed' as GroupStatus,
            failed_tasks: event.failed_tasks ?? 0,
          })
          // 失败后关闭连接并刷新
          eventSources.delete(groupId)
          eventSource.close()
          loadGroups()
          break
      }
    })

    eventSources.set(groupId, eventSource)
  }

  /**
   * 从服务端获取批量处理上限
   */
  async function fetchBatchLimit() {
    try {
      batchLimit.value = await api.getBatchLimit()
    } catch (err) {
      console.warn('Failed to fetch batch limit, using default:', err)
      // 保持默认值 100
    }
  }

  // ── 轮询兜底（SSE 失败时） ────────────────────────────────────────

  let pollTimer: ReturnType<typeof setInterval> | null = null

  function startPolling() {
    stopPolling()
    pollTimer = setInterval(() => {
      loadGroups()
    }, 5000)
  }

  function stopPolling() {
    if (pollTimer) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  }

  // ── 下载 ──────────────────────────────────────────────────────────

  /**
   * 下载分组所有已完成音频为ZIP
   */
  async function downloadGroupAudio(groupId: string) {
    error.value = null
    downloadingGroupId.value = groupId
    try {
      const blob = await api.downloadGroupAudio(groupId)
      const group = groupMap.value.get(groupId)
      const filename = group ? `${group.name}.zip` : `${groupId}.zip`

      // Trigger download
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = filename
      document.body.appendChild(a)
      a.click()
      document.body.removeChild(a)
      URL.revokeObjectURL(url)
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : '下载失败'
      error.value = message
      console.error('Failed to download group audio:', err)
      throw err
    } finally {
      downloadingGroupId.value = null
    }
  }

  // ── Reset ─────────────────────────────────────────────────────

  function resetStore() {
    groupMap.value = new Map()
    groupTaskCache.clear()
    currentPage.value = 0
    totalCount.value = 0
    error.value = null
    loading.value = false
    refreshing.value = false
  }

  return {
    // State
    groupMap,
    loading,
    refreshing,
    error,
    batchLimit,
    downloadingGroupId,
    currentPage,
    perPage,
    totalCount,
    hasMore,

    // Computed
    allGroups,
    activeGroups,
    completedGroups,
    failedGroups,

    // For backward compat: alias groups to allGroups
    groups: allGroups,

    // Group task cache (exposed for GroupDetailPanel)
    groupTaskCache,

    // Paginated load
    loadGroups,
    loadPage,
    loadMore,
    loadGroupsPage,
    loadMoreGroups,

    // Group detail / tasks
    getGroupDetailWithTasks,
    loadGroupTasks,
    getGroupTasks,
    loadMoreGroupTasks,

    // CRUD
    importBatch,
    updateGroup,
    pauseGroup,
    resumeGroup,
    retryFailed,
    removeGroup,
    downloadGroupAudio,

    // SSE
    subscribeToGroupEvents,
    fetchBatchLimit,
    startPolling,
    stopPolling,

    // Reset
    resetStore,
  }
})
