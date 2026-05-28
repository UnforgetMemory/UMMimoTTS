import axios from 'axios'

const apiClient = axios.create({
  baseURL: '',
  timeout: 120000,
  headers: {
    'Content-Type': 'application/json',
  },
})

export interface Voice {
  id: string
  name: string
  language: string
  gender: string
  style: string
  preview_url?: string  // 试听音频 URL
}

export type TaskStatus = 'pending' | 'queued' | 'synthesizing' | 'streaming' | 'completed' | 'failed' | 'cancelled'

export interface Task {
  id: string
  custom_title?: string
  status: TaskStatus
  model: string
  voice: string | null
  text: string
  context?: string | null
  created_at: string
  completed_at: string | null
  error: string | null
  progress: number
  token_count: number
  char_count: number
  elapsed_secs: number | null
  has_audio: boolean
  // 分片进度信息
  total_chunks?: number
  current_chunk?: number
  /** 所属批量分组 ID */
  group_id?: string | null
}

// ── Paginated / Summary types ────────────────────────────────────────

/** Lightweight list item - NO text/context/model fields */
export interface TaskSummary {
  id: string
  custom_title?: string
  status: TaskStatus
  voice: string | null
  char_count: number
  token_count: number
  progress: number
  has_audio: boolean
  group_id?: string | null
  created_at: string
  completed_at: string | null
  elapsed_secs: number | null
  current_chunk?: number
  total_chunks?: number
}

/** Group summary - NO tasks array embedded */
export interface GroupSummary {
  id: string
  name: string
  status: GroupStatus
  voice: string | null
  model: string
  context: string | null
  created_at: string
  total_tasks: number
  completed_tasks: number
  failed_tasks: number
  total_tokens: number
}

export interface PaginatedResponse<T> {
  items: T[]
  total: number
  page: number
  per_page: number
  total_pages: number
}

export interface StatsSummary {
  total_tasks: number
  completed: number
  failed: number
  processing: number
  total_tokens: number
  total_chars: number
}

export interface TaskListParams {
  page?: number
  per_page?: number
  status?: TaskStatus
  search?: string
  sort?: string
  group_id?: string
}

// ── Batch Import v2 (token-based backend cache) ────────────────

export interface BatchImportItem {
  index: number
  text: string
  text_preview: string
  voice: string | null
  model: string | null
  title: string | null
  context: string | null
  char_count: number
  token_count: number
  source_filename: string | null
  has_error: boolean
  error: string | null
}

export interface FileStat {
  filename: string
  item_count: number
  char_count: number
  token_count: number
}

export interface BatchUploadResponse {
  token: string
  stats: BatchImportStats
  file_stats: FileStat[]
}

export interface BatchImportStats {
  total_items: number
  valid_items: number
  error_items: number
  total_chars: number
  total_token_count: number
  file_stats: FileStat[]
  created_at: string
  expires_at: string
}

export interface BatchImportSubmitResponse {
  group_id: string
  task_count: number
  task_ids: string[]
}

export interface BatchSubmitConfig {
  group_name?: string
  default_voice?: string
  default_model?: string
  default_context?: string
  default_speed?: number
}

// ── Existing types (kept for detail / backward compat) ──────────────

export interface SynthesizeRequest {
  text: string
  voice: string
  model: string
  context?: string
  task_name?: string  // Optional custom task name
  api_key?: string
}

export interface TaskEvent {
  task_id: string
  event_type: 'status_changed' | 'completed' | 'failed'
  status?: TaskStatus
  progress?: number
  error?: string
}

// ── Batch / Group types ──────────────────────────────────────────────

export type GroupStatus = 'pending' | 'processing' | 'paused' | 'completed' | 'failed' | 'cancelled'

/** 批量分组 (full detail) */
export interface BatchGroup {
  id: string
  name: string
  status: GroupStatus
  voice: string | null
  model: string
  context: string | null
  created_at: string
  task_ids: string[]
  total_tasks: number
  completed_tasks: number
  failed_tasks: number
  total_tokens: number
  tasks?: Task[]
}

/** 更新分组请求 */
export interface GroupUpdateRequest {
  name?: string
  voice?: string
  model?: string
  context?: string
}

/** 单任务覆盖配置 */
export interface TaskConfig {
  task_name?: string
  voice?: string
  model?: string
  context?: string
}

/** 批量导入请求（通过 FormData 上传） */
export interface BatchImportRequest {
  files: File[]
  group_name?: string
  voice?: string
  model?: string
  context?: string
  task_configs?: TaskConfig[]
  use_filename_as_task_name?: boolean
  api_key?: string
}

/** 批量导入响应 */
export interface BatchCreateResponse {
  group_id: string
  group_name: string
  task_count: number
  tasks: Task[]
}

// ── Batch Import (token-based, backend-parsed) ──────────────────────

/** A single parsed item from the backend cache */
export interface ParsedItem {
  index: number
  text_preview: string
  voice?: string | null
  model?: string | null
  title?: string | null
  context?: string | null
  char_count: number
  has_error: boolean
  error: string | null
  source_filename?: string | null
  token_count: number
}

/** Per-item override payload */
export interface ItemOverride {
  voice?: string
  model?: string
  context?: string
  title?: string
}

/** Batch submit payload */
export interface BatchSubmitRequest {
  default_voice: string
  default_model: string
  default_context: string
  group_name?: string
}

/** Batch submit response */
export interface BatchSubmitResponse {
  group_id: string
  task_count: number
}

/** 分组列表响应 */
export interface GroupListResponse {
  groups: BatchGroup[]
  total: number
}

/** 分组 SSE 事件 */
export interface GroupEvent {
  group_id: string
  event_type: 'task_completed' | 'task_failed' | 'group_completed' | 'group_failed' | 'progress'
  task_id?: string
  status?: GroupStatus
  group_status?: GroupStatus
  progress?: number
  completed_tasks?: number
  failed_tasks?: number
  total_tasks?: number
  error?: string
}

export const api = {
  async getVoices(): Promise<Voice[]> {
    const response = await apiClient.get('/api/v1/voices')
    return response.data.voices
  },

  async synthesize(request: SynthesizeRequest): Promise<{ task_id: string }> {
    const response = await apiClient.post('/api/v1/tts/synthesize', request)
    return response.data
  },

  // ── Paginated API (new) ──────────────────────────────────────────

  /**
   * Get paginated task summaries for list display
   * Frontend uses 0-based pages; backend expects 1-based pages.
   * GET /api/v1/tasks?page=1&per_page=50&status=...&search=...&sort=created_at
   */
  async getTasksPaginated(params: TaskListParams = {}): Promise<PaginatedResponse<TaskSummary>> {
    const response = await apiClient.get('/api/v1/tasks', { params })
    return response.data
  },

  /**
   * Get paginated group summaries
   * Frontend uses 0-based pages; backend expects 1-based pages.
   * GET /api/v1/groups?page=1&per_page=20
   */
  async getGroupsPaginated(page = 0, perPage = 20): Promise<PaginatedResponse<GroupSummary>> {
    const response = await apiClient.get('/api/v1/groups', { params: { page, per_page: perPage } })
    return response.data
  },

  /**
   * Get paginated tasks for a specific group
   * Frontend uses 0-based pages; backend expects 1-based pages.
   * GET /api/v1/groups/{id}/tasks?page=1&per_page=50
   */
  async getGroupTasks(groupId: string, page = 0, perPage = 50): Promise<PaginatedResponse<TaskSummary>> {
    const response = await apiClient.get(`/api/v1/groups/${groupId}/tasks`, { params: { page, per_page: perPage } })
    return response.data
  },

  /**
   * Get stats summary
   * GET /api/v1/stats/summary
   */
  async getStatsSummary(): Promise<StatsSummary> {
    const response = await apiClient.get('/api/v1/stats/summary')
    return response.data
  },

  /**
   * Get stats for a specific group
   * GET /api/v1/groups/{id}/stats
   */
  async getGroupStats(groupId: string): Promise<StatsSummary> {
    const response = await apiClient.get(`/api/v1/groups/${groupId}/stats`)
    return response.data
  },

  // ── Batch Import v2 API ─────────────────────────────────────────

  // ── Batch Import v2 (token-based backend cache) ───────────--

  /**
   * Upload file for batch import – backend parses, caches, returns token.
   * POST /api/v1/batch/upload
   */
  async uploadBatchFile(file: File, onProgress?: (pct: number) => void): Promise<BatchUploadResponse> {
    const formData = new FormData()
    formData.append('file', file)

    const response = await apiClient.post('/api/v1/batch/upload', formData, {
      headers: { 'Content-Type': undefined },
      timeout: 300000,
      onUploadProgress(progressEvent) {
        if (onProgress && progressEvent.total) {
          onProgress(Math.round((progressEvent.loaded / progressEvent.total) * 100))
        }
      },
    })
    return response.data
  },

  /**
   * Get paginated preview of parsed items.
   * GET /api/v1/batch/preview?token=xxx&page=0&per_page=50
   */
  async getBatchImportPreview(token: string, page = 0, perPage = 50): Promise<PaginatedResponse<ParsedItem>> {
    const response = await apiClient.get('/api/v1/batch/preview', {
      params: { token, page, per_page: perPage },
    })
    return response.data
  },

  /**
   * Override a single parsed item.
   * PUT /api/v1/batch/items/{index}
   */
  async updateBatchImportItem(token: string, index: number, overrides: ItemOverride): Promise<ParsedItem> {
    const response = await apiClient.put(`/api/v1/batch/items/${index}`, { token, ...overrides })
    return response.data
  },

  /**
   * Extend token TTL (must be called every ~4 min).
   * POST /api/v1/batch/extend
   */
  async extendBatchImportSession(token: string): Promise<{ status: string }> {
    const response = await apiClient.post('/api/v1/batch/extend', { token })
    return response.data
  },

  /**
   * Get paginated per-file statistics for a batch import.
   * GET /api/v1/batch/files?token=xxx&sort=filename&dir=asc&page=0&per_page=20
   */
  async getBatchImportFiles(
    token: string,
    options?: { sort?: string; dir?: string; page?: number; per_page?: number },
  ): Promise<PaginatedResponse<FileStat>> {
    const response = await apiClient.get('/api/v1/batch/files', {
      params: {
        token,
        sort: options?.sort ?? 'filename',
        dir: options?.dir ?? 'asc',
        page: options?.page ?? 0,
        per_page: options?.per_page ?? 20,
      },
    })
    return response.data
  },

  /**
   * Remove all items from a specific file in a batch import.
   * DELETE /api/v1/batch/files/{filename}?token=xxx
   */
  async removeBatchImportFile(token: string, filename: string): Promise<{ removed_count: number }> {
    const response = await apiClient.delete(`/api/v1/batch/files/${encodeURIComponent(filename)}`, {
      params: { token },
    })
    return response.data
  },

  /**
   * Submit batch import – creates Group + Tasks.
   * POST /api/v1/batch/submit
   */
  async submitBatchImport(token: string, config: BatchSubmitRequest): Promise<BatchSubmitResponse> {
    const response = await apiClient.post('/api/v1/batch/submit', { token, ...config })
    return response.data
  },

  // ── Existing API (kept for backward compat) ─────────────────────

  /** @deprecated Use getTasksPaginated instead */
  async getTasks(): Promise<Task[]> {
    const response = await apiClient.get('/api/v1/tasks')
    return response.data
  },

  async getTask(taskId: string): Promise<Task> {
    const response = await apiClient.get(`/api/v1/tasks/${taskId}`)
    return response.data
  },

  async deleteTask(taskId: string): Promise<void> {
    await apiClient.delete(`/api/v1/tasks/${taskId}`)
  },

  getAudioUrl(taskId: string): string {
    return `/api/v1/tasks/${taskId}/audio`
  },

  async downloadTaskAudio(taskId: string): Promise<Blob> {
    const response = await apiClient.get(`/api/v1/tasks/${taskId}/download`, {
      responseType: 'blob',
    })
    return response.data
  },

  subscribeToTask(taskId: string, onEvent: (event: TaskEvent) => void): EventSource {
    const eventSource = new EventSource(`/api/v1/sse/tasks/${taskId}`)
    
    // 处理命名事件（后端用 event: xxx 推送）
    eventSource.addEventListener('status_changed', (event) => {
      try {
        const data = JSON.parse(event.data)
        onEvent(data)
      } catch (error) {
        console.error('Failed to parse status_changed event:', error)
      }
    })
    
    eventSource.addEventListener('completed', (event) => {
      try {
        const data = JSON.parse(event.data)
        onEvent(data)
      } catch (error) {
        console.error('Failed to parse completed event:', error)
      }
    })
    
    eventSource.addEventListener('failed', (event) => {
      try {
        const data = JSON.parse(event.data)
        onEvent(data)
      } catch (error) {
        console.error('Failed to parse failed event:', error)
      }
    })
    
    // 兼容匿名消息（发送纯 data: 的情况，如 connected）
    eventSource.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        // 只处理匿名消息，命名事件已由 addEventListener 处理
        if (!data.event_type) {
          onEvent(data)
        }
      } catch (error) {
        console.error('Failed to parse SSE message:', error)
      }
    }
    
    // 出错时不 close()，让 EventSource 自动重连
    eventSource.onerror = () => {
      console.warn('SSE connection error for task, will auto-reconnect:', taskId)
    }
    
    return eventSource
  },

  // 获取音色试听 URL
  // 优先使用 CDN URL（零延迟），回退到后端代理（兼容性）
  getVoicePreviewUrl(voiceId: string, previewUrl?: string): string {
    return previewUrl || `/api/v1/voices/${voiceId}/preview`
  },

  async updateTaskTitle(taskId: string, title: string): Promise<void> {
    await apiClient.patch(`/api/v1/tasks/${taskId}/title`, { title })
  },

  // ── Batch / Group API ────────────────────────────────────────────

  /**
   * 批量导入文件并创建分组
   * 使用 FormData 上传多个音频/文本文件
   */
  async importBatch(request: BatchImportRequest): Promise<BatchCreateResponse> {
    const formData = new FormData()
    for (const file of request.files) {
      formData.append('files', file)
    }
    if (request.group_name) formData.append('group_name', request.group_name)
    if (request.voice) formData.append('voice', request.voice)
    if (request.model) formData.append('model', request.model)
    if (request.context) formData.append('context', request.context)
    if (request.task_configs) formData.append('task_configs', JSON.stringify(request.task_configs))
    if (request.use_filename_as_task_name !== undefined) {
      formData.append('use_filename_as_task_name', String(request.use_filename_as_task_name))
    }
    if (request.api_key) formData.append('api_key', request.api_key)

    const response = await apiClient.post('/api/v1/batch/import', formData, {
      headers: { 'Content-Type': undefined },  // let browser set multipart boundary
      timeout: 300000, // 5 min for large uploads
    })
    return response.data
  },

  /** @deprecated Use getGroupsPaginated instead */
  async getGroups(): Promise<GroupListResponse> {
    const response = await apiClient.get('/api/v1/groups')
    return response.data
  },

  /** 获取单个分组详情 */
  async getGroup(groupId: string): Promise<BatchGroup> {
    const response = await apiClient.get(`/api/v1/groups/${groupId}`)
    return response.data
  },

  /** 更新分组设置（名称、音色、模型、上下文） */
  async updateGroup(groupId: string, request: GroupUpdateRequest): Promise<BatchGroup> {
    const response = await apiClient.patch(`/api/v1/groups/${groupId}`, request)
    return response.data
  },

  /** 删除分组及其关联任务 */
  async deleteGroup(groupId: string): Promise<void> {
    await apiClient.delete(`/api/v1/groups/${groupId}`)
  },

  /**
   * 订阅分组 SSE 事件
   * 事件类型：task_completed, task_failed, group_completed, group_failed, progress
   */
  subscribeToGroup(groupId: string, onEvent: (event: GroupEvent) => void): EventSource {
    const eventSource = new EventSource(`/api/v1/sse/groups/${groupId}`)

    // 命名事件
    const namedEvents = ['task_completed', 'task_failed', 'group_completed', 'group_failed', 'progress'] as const
    for (const eventName of namedEvents) {
      eventSource.addEventListener(eventName, (event) => {
        try {
          const data = JSON.parse(event.data)
          onEvent(data)
        } catch (error) {
          console.error(`Failed to parse ${eventName} event:`, error)
        }
      })
    }

    // 匿名消息兜底
    eventSource.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        if (!data.event_type) {
          onEvent(data)
        }
      } catch (error) {
        console.error('Failed to parse group SSE message:', error)
      }
    }

    // 出错时让 EventSource 自动重连
    eventSource.onerror = () => {
      console.warn('SSE connection error for group, will auto-reconnect:', groupId)
    }

    return eventSource
  },

  // ── New paginated API methods ─────────────────────────────────────

  /**
   * Get single group detail (summary)
   * GET /api/v1/groups/{id}
   */
  async getGroupDetail(groupId: string): Promise<GroupSummary> {
    const response = await apiClient.get(`/api/v1/groups/${groupId}`)
    return response.data
  },

  /**
   * Get group detail with paginated tasks
   * GET /api/v1/groups/{id}?page=X&per_page=Y
   */
  async getGroupDetailWithTasks(groupId: string, page = 0, perPage = 50): Promise<{ group: GroupSummary, tasks: PaginatedResponse<TaskSummary> }> {
    const response = await apiClient.get(`/api/v1/groups/${groupId}`, { params: { page: page + 1, per_page: perPage } })
    return response.data
  },

  /**
   * Get global stats
   * GET /api/v1/stats
   */
  async getStats(): Promise<StatsSummary> {
    const response = await apiClient.get('/api/v1/stats')
    return response.data
  },

  /** 获取服务端批量处理上限 */
  async getBatchLimit(): Promise<number> {
    const response = await apiClient.get('/api/v1/batch/limit')
    return response.data.limit
  },

  /** 暂停分组处理 */
  async pauseGroup(groupId: string): Promise<void> {
    await apiClient.post(`/api/v1/groups/${groupId}/pause`)
  },

  /** 恢复分组处理 */
  async resumeGroup(groupId: string): Promise<void> {
    await apiClient.post(`/api/v1/groups/${groupId}/resume`)
  },

  /** 重试分组中失败的任务 */
  async retryFailed(groupId: string): Promise<void> {
    await apiClient.post(`/api/v1/groups/${groupId}/retry-failed`)
  },

  /** 下载分组所有已完成音频为ZIP */
  async downloadGroupAudio(groupId: string): Promise<Blob> {
    const response = await apiClient.get(`/api/v1/groups/${groupId}/download`, {
      responseType: 'blob',
    })
    return response.data
  },

}

// ── V2 API Response Types (internal) ─────────────────────────────

interface TaskV2Response {
  id: string
  task_type: string
  status: TaskStatus
  batch_id?: string
  group_id?: string
  content: string
  content_ref?: string
  title?: string
  voice?: string
  model?: string
  style?: string
  speed?: number
  total_chars: number
  total_tokens: number
  total_chunks: number
  done_chunks: number
  failed_chunks: number
  output_path?: string
  audio_duration?: number
  max_retries?: number
  retry_count?: number
  created_at: string
  updated_at: string
  completed_at?: string | null
}

interface BatchV2Item {
  seq: number
  text: string
  status: string
  priority?: number
  error?: string
}

interface BatchV2TaskSummary {
  id: string
  task_type: string
  status: TaskStatus
  content: string
  title?: string
  voice?: string
  model?: string
  total_chunks: number
  done_chunks: number
  output_path?: string
  created_at: string
  updated_at: string
  completed_at?: string | null
}

interface BatchV2Response {
  id: string
  title: string
  status: GroupStatus
  voice?: string
  model?: string
  speed?: number
  total_items: number
  total_chars: number
  total_tokens: number
  items?: BatchV2Item[]
  tasks?: BatchV2TaskSummary[]
  created_at: string
  updated_at: string
}

interface PaginatedV2Response<T> {
  data: T[]
  page: number
  page_size: number
  total: number
}

// ── V2 Request Params (exported) ─────────────────────────────────

export interface CreateTaskV2Params {
  content: string
  task_type?: string
  voice?: string
  model?: string
  style?: string
  speed?: number
  title?: string
  batch_id?: string
  group_id?: string
}

export interface ListTasksV2Params {
  page?: number
  page_size?: number
  status?: TaskStatus
  search?: string
  sort?: string
  group_id?: string
  batch_id?: string
}

export interface CreateBatchV2Params {
  title: string
  voice?: string
  model?: string
  style?: string
  speed?: number
  items?: Array<{ seq: number; text: string; priority?: number }>
}

// ── Transform helpers ─────────────────────────────────────────────

function transformV2Task(v2: TaskV2Response): Task {
  let error: string | null = null
  if (v2.status === 'failed') {
    error = `Task failed after ${v2.failed_chunks} chunk(s)`
  }

  const startTime = new Date(v2.created_at).getTime()
  const endTime = v2.completed_at
    ? new Date(v2.completed_at).getTime()
    : new Date(v2.updated_at).getTime()

  return {
    id: v2.id,
    custom_title: v2.title,
    status: v2.status,
    model: v2.model || '',
    voice: v2.voice || null,
    text: v2.content,
    context: null,
    created_at: v2.created_at,
    completed_at: v2.completed_at || null,
    error,
    progress: v2.total_chunks > 0 ? v2.done_chunks / v2.total_chunks : 0,
    token_count: v2.total_tokens,
    char_count: v2.total_chars,
    elapsed_secs: Math.round((endTime - startTime) / 1000),
    has_audio: !!v2.output_path,
    total_chunks: v2.total_chunks,
    current_chunk: v2.done_chunks,
    group_id: v2.group_id || null,
  }
}

function transformV2BatchTask(v2: BatchV2TaskSummary): Task {
  const startTime = new Date(v2.created_at).getTime()
  const endTime = v2.completed_at
    ? new Date(v2.completed_at).getTime()
    : new Date(v2.updated_at).getTime()

  return {
    id: v2.id,
    custom_title: v2.title,
    status: v2.status,
    model: v2.model || '',
    voice: v2.voice || null,
    text: v2.content,
    context: null,
    created_at: v2.created_at,
    completed_at: v2.completed_at || null,
    error: v2.status === 'failed' ? 'Task failed' : null,
    progress: v2.total_chunks > 0 ? v2.done_chunks / v2.total_chunks : 0,
    token_count: 0,
    char_count: 0,
    elapsed_secs: Math.round((endTime - startTime) / 1000),
    has_audio: !!v2.output_path,
    total_chunks: v2.total_chunks,
    current_chunk: v2.done_chunks,
    group_id: null,
  }
}

function transformV2Batch(v2: BatchV2Response): BatchGroup {
  const tasks = v2.tasks || []
  const completedTasks = tasks.filter(t => t.status === 'completed').length
  const failedTasks = tasks.filter(t => t.status === 'failed').length

  return {
    id: v2.id,
    name: v2.title,
    status: v2.status,
    voice: v2.voice || null,
    model: v2.model || '',
    context: null,
    created_at: v2.created_at,
    task_ids: tasks.map(t => t.id),
    total_tasks: v2.total_items || tasks.length,
    completed_tasks: completedTasks,
    failed_tasks: failedTasks,
    total_tokens: v2.total_tokens,
    tasks: tasks.map(transformV2BatchTask),
  }
}

function convertPaginated<T>(v2: PaginatedV2Response<T>): PaginatedResponse<T> {
  return {
    items: v2.data,
    total: v2.total,
    page: v2.page,
    per_page: v2.page_size,
    total_pages: Math.ceil(v2.total / v2.page_size),
  }
}

// ── V2 API Client ─────────────────────────────────────────────────

export const apiV2 = {
  // ── Tasks ──────────────────────────────────────────────────────

  async createTask(params: CreateTaskV2Params): Promise<Task> {
    const response = await apiClient.post('/api/v2/tasks', params)
    return transformV2Task(response.data)
  },

  async listTasks(params: ListTasksV2Params = {}): Promise<PaginatedResponse<Task>> {
    const response = await apiClient.get('/api/v2/tasks', { params })
    const data = response.data
    // Backend returns either a plain array or { data: [], total, page, page_size }
    if (Array.isArray(data)) {
      const items = data.map(transformV2Task)
      return {
        items,
        total: items.length,
        page: 0,
        per_page: items.length || 50,
        total_pages: 1,
      }
    }
    const v2 = data as PaginatedV2Response<TaskV2Response>
    return {
      items: (v2.data || []).map(transformV2Task),
      total: v2.total || 0,
      page: v2.page || 0,
      per_page: v2.page_size || 50,
      total_pages: Math.ceil((v2.total || 0) / (v2.page_size || 50)),
    }
  },

  async getTask(id: string): Promise<Task> {
    const response = await apiClient.get(`/api/v2/tasks/${id}`)
    return transformV2Task(response.data)
  },

  async enqueueTask(id: string): Promise<void> {
    await apiClient.post(`/api/v2/tasks/${id}/enqueue`)
  },

  async retryTask(id: string): Promise<void> {
    await apiClient.post(`/api/v2/tasks/${id}/retry`)
  },

  async continueTask(id: string): Promise<void> {
    await apiClient.post(`/api/v2/tasks/${id}/continue`)
  },

  // ── Batches ──────────────────────────────────────────────────

  async createBatch(params: CreateBatchV2Params): Promise<BatchGroup> {
    const response = await apiClient.post('/api/v2/batches', params)
    return transformV2Batch(response.data)
  },

  async getBatch(id: string): Promise<BatchGroup> {
    const response = await apiClient.get(`/api/v2/batches/${id}`)
    return transformV2Batch(response.data)
  },

  async addBatchItem(batchId: string, item: { seq: number; filename: string; content: string }): Promise<void> {
    await apiClient.post(`/api/v2/batches/${batchId}/items`, item)
  },

  async addBatchItems(batchId: string, items: Array<{ seq: number; filename: string; content: string }>): Promise<{ ok: boolean; count: number }> {
    const response = await apiClient.post(`/api/v2/batches/${batchId}/items/batch`, items)
    return response.data
  },

  async updateBatchItem(batchId: string, seq: number, item: { text: string; priority?: number }): Promise<void> {
    await apiClient.put(`/api/v2/batches/${batchId}/items/${seq}`, item)
  },

  async deleteBatchItem(batchId: string, seq: number): Promise<void> {
    await apiClient.delete(`/api/v2/batches/${batchId}/items/${seq}`)
  },

  async deleteBatch(id: string): Promise<void> {
    await apiClient.delete(`/api/v2/batches/${id}`)
  },

  async submitBatch(id: string): Promise<BatchGroup> {
    const response = await apiClient.post(`/api/v2/batches/${id}/submit`)
    return transformV2Batch(response.data)
  },

  // ── Groups ────────────────────────────────────────────────────

  async createGroup(params: { name: string; batch_ids: string[] }): Promise<GroupSummary> {
    const response = await apiClient.post('/api/v2/groups', params)
    return response.data
  },

  async listGroups(params: { batch_id?: string; page?: number; page_size?: number } = {}): Promise<PaginatedResponse<GroupSummary>> {
    const response = await apiClient.get('/api/v2/groups', { params })
    const data = response.data
    // Backend returns either a plain array or { data: [], total, page, page_size }
    if (Array.isArray(data)) {
      return {
        items: data,
        total: data.length,
        page: 0,
        per_page: data.length || 20,
        total_pages: 1,
      }
    }
    return convertPaginated(data)
  },
}
