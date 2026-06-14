import axios, { type AxiosError } from 'axios'
import { toast } from 'vue-sonner'
import { handleNetworkError } from '../utils/errorHandler'

const apiClient = axios.create({
  baseURL: '',
  timeout: 120000,
  headers: {
    'Content-Type': 'application/json',
  },
})

apiClient.interceptors.response.use(
  (response) => response,
  (error: AxiosError<{ error?: string }>) => {
    if (!error.response) {
      handleNetworkError()
      return Promise.reject({ message: '网络连接失败', code: 'NETWORK_ERROR' })
    }

    const message = error.response.data?.error || error.message || '请求失败'
    toast.error(message)
    return Promise.reject({ message, code: error.response.status })
  },
)

export interface Voice {
  id: string
  name: string
  language: string
  gender: string
  style: string
  preview_url?: string  // 试听音频 URL
}

export interface VoicePreset {
  id: string
  name: string
  language: string
  gender: string
  style: string
  preview_url: string
}

export interface ModelPreset {
  id: string
  name: string
  description: string
}

export interface ProviderInfo {
  id: string
  name: string
  base_url: string
  api_key?: string  // Present only when explicitly returned (e.g. after PUT)
  is_configured: boolean
  is_default: boolean
  created_at: string
  updated_at: string
}

export interface AppConfig {
  voices: VoicePreset[]
  models: ModelPreset[]
  default_voice: string
  default_model: string
  default_speed: number
  mimo_base_url: string
  providers: ProviderInfo[]
}

export async function fetchConfig(): Promise<AppConfig> {
  const resp = await fetch('/api/v2/config')
  if (!resp.ok) throw new Error(`Failed to fetch config: ${resp.status}`)
  return resp.json()
}

export type TaskStatus = 'pending' | 'queued' | 'chunking' | 'processing' | 'merging' | 'mergingfailed' | 'paused' | 'done' | 'failed' | 'cancelled'

export interface Task {
  id: string
  custom_title?: string
  status: TaskStatus
  model: string
  voice: string | null
  text: string
  context?: string | null
  provider_id?: string
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
  title?: string
  status: TaskStatus
  voice: string | null
  provider_id?: string
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
  progress?: number  // 0-100, calculated from completed_tasks/total_tasks
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

export type GroupStatus = 'pending' | 'queued' | 'preparing' | 'processing' | 'paused' | 'completed' | 'failed' | 'cancelled'

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
  // ── Task API ─────────────────────────────────────────────────────

  async deleteTask(taskId: string): Promise<void> {
    await apiClient.delete(`/api/v2/tasks/${taskId}`)
  },

  async clearAllTasks(): Promise<void> {
    await apiClient.delete('/api/v2/tasks/clear')
  },

  getAudioUrl(taskId: string): string {
    return `/api/v2/tasks/${taskId}/audio`
  },

  subscribeToTask(taskId: string, onEvent: (event: TaskEvent) => void): EventSource {
    // V2 SSE: /api/v2/events?channel=task:{id}
    // V2 backend sends DomainEvent JSON with `type` discriminator
    const eventSource = new EventSource(`/api/v2/events?channel=task:${taskId}`)
    
    // V2 DomainEvents use `type` field for event type discrimination
    // Map V2 DomainEvent types to TaskEvent format
    eventSource.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        const eventType = data.type
        
        if (eventType === 'TaskCompleted') {
          onEvent({
            task_id: data.task_id,
            event_type: 'completed',
            status: 'done',
            progress: 1.0,
          })
        } else if (eventType === 'TaskFailed') {
          onEvent({
            task_id: data.task_id,
            event_type: 'failed',
            status: 'failed',
            error: data.error,
          })
        } else if (eventType === 'AllChunksDone') {
          onEvent({
            task_id: data.task_id,
            event_type: 'status_changed',
            status: 'merging',
            progress: 1.0,
          })
        } else if (eventType === 'ChunkCompleted') {
          onEvent({
            task_id: data.task_id,
            event_type: 'status_changed',
            status: 'processing',
            progress: data.seq / (data.total_chunks || 10),
          })
        } else if (eventType === 'TaskEnqueued') {
          onEvent({
            task_id: data.task_id,
            event_type: 'status_changed',
            status: 'queued',
            progress: 0,
          })
        } else if (eventType === 'TaskStatusChanged') {
          // Real-time status transitions (cancelled, paused, etc.)
          onEvent({
            task_id: data.task_id,
            event_type: 'status_changed',
            status: data.status ?? 'processing',
            progress: data.progress ?? undefined,
          })
        } else if (eventType === 'ChunkFailed') {
          onEvent({
            task_id: data.task_id,
            event_type: 'status_changed',
            status: 'processing',
            progress: data.seq / (data.total_chunks || 10),
          })
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

  /** Subscribe to batch/group events via V2 SSE */
  subscribeToChannel(channel: string, onEvent: (event: any) => void): EventSource {
    const eventSource = new EventSource(`/api/v2/events?channel=${channel}`)
    
    eventSource.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        onEvent(data)
      } catch (error) {
        console.error('Failed to parse SSE message:', error)
      }
    }
    
    eventSource.onerror = () => {
      console.warn('SSE connection error for channel, will auto-reconnect:', channel)
    }
    
    return eventSource
  },

  async updateTaskTitle(taskId: string, title: string): Promise<void> {
    await apiClient.patch(`/api/v2/tasks/${taskId}/title`, { title })
  },

  // ── Batch / Group API ────────────────────────────────────────────

  /** 更新分组设置（名称、音色、模型、上下文） */
  async updateGroup(groupId: string, request: GroupUpdateRequest): Promise<BatchGroup> {
    const response = await apiClient.patch(`/api/v2/batches/${groupId}`, request)
    return response.data
  },

  /** 获取服务端批量处理上限 */
  async getBatchLimit(): Promise<number> {
    const response = await apiClient.get('/api/v2/batches/limit')
    return response.data.limit
  },

  /** 暂停分组处理 */
  async pauseGroup(groupId: string): Promise<void> {
    await apiClient.post(`/api/v2/batches/${groupId}/pause`)
  },

  /** 恢复分组处理 */
  async resumeGroup(groupId: string): Promise<void> {
    await apiClient.post(`/api/v2/batches/${groupId}/resume`)
  },

  /** 重试分组中失败的任务 */
  async retryFailed(groupId: string): Promise<void> {
    await apiClient.post(`/api/v2/batches/${groupId}/retry-failed`)
  },

  /** 取消分组处理 */
  async cancelGroup(groupId: string): Promise<void> {
    await apiClient.post(`/api/v2/batches/${groupId}/cancel`)
  },

  /** 下载分组所有已完成音频为ZIP */
  async downloadGroupAudio(groupId: string): Promise<Blob> {
    const response = await apiClient.get(`/api/v2/batches/${groupId}/download`, {
      responseType: 'blob',
    })
    return response.data
  },

}

// ── V2 API Response Types (internal) ─────────────────────────────

interface TaskV2Response {
  id: string
  task_type: string
  /** Backend sends capitalized Debug format (e.g. "Pending", "Done") — normalized via normalizeBackendStatus() */
  status: string
  batch_id?: string
  group_id?: string
  provider_id?: string
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
  /** Backend sends capitalized Debug format — normalized via normalizeBackendStatus() */
  status: string
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
  group_id?: string | null
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
  provider_id?: string
}

export interface ListTasksV2Params {
  page?: number
  page_size?: number
  status?: TaskStatus
  search?: string
  sort?: string
  group_id?: string
  batch_id?: string
  standalone?: boolean
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

/**
 * Normalize backend TaskStatus (capitalized Debug format) to frontend TaskStatus (lowercase).
 *
 * Backend variants: Pending, Queued, Chunking, Processing, Merging, MergingFailed, Paused, Done, Failed, Cancelled
 * Frontend types: pending, queued, chunking, processing, merging, mergingfailed, paused, done, failed, cancelled
 */
function normalizeBackendStatus(raw: string): TaskStatus {
  const validStatuses: TaskStatus[] = [
    'pending', 'queued', 'chunking', 'processing', 'merging',
    'mergingfailed', 'paused', 'done', 'failed', 'cancelled'
  ]
  const lower = raw.toLowerCase() as TaskStatus
  if (validStatuses.includes(lower)) return lower
  return 'pending'
}

function transformV2Task(v2: TaskV2Response): Task {
  const normalizedStatus = normalizeBackendStatus(v2.status)
  let error: string | null = null
  if (normalizedStatus === 'failed') {
    const errParts: string[] = []
    if (v2.total_chunks > 0) {
      errParts.push(`分片进度 ${v2.done_chunks}/${v2.total_chunks}`)
    }
    if (v2.failed_chunks > 0) {
      errParts.push(`失败 ${v2.failed_chunks} 个`)
    }
    if (v2.retry_count != null && v2.retry_count > 0) {
      errParts.push(`已重试 ${v2.retry_count}/${v2.max_retries ?? '?'} 次`)
    }
    error = errParts.length > 0 ? `合成失败：${errParts.join('，')}` : '合成失败'
  }

  const startTime = new Date(v2.created_at).getTime()
  const endTime = v2.completed_at
    ? new Date(v2.completed_at).getTime()
    : new Date(v2.updated_at).getTime()

  return {
    id: v2.id,
    custom_title: v2.title,
    status: normalizedStatus,
    model: v2.model || '',
    voice: v2.voice || null,
    provider_id: v2.provider_id,
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
  const normalizedStatus = normalizeBackendStatus(v2.status)
  const startTime = new Date(v2.created_at).getTime()
  const endTime = v2.completed_at
    ? new Date(v2.completed_at).getTime()
    : new Date(v2.updated_at).getTime()

  return {
    id: v2.id,
    custom_title: v2.title,
    status: normalizedStatus,
    model: v2.model || '',
    voice: v2.voice || null,
    text: v2.content,
    context: null,
    created_at: v2.created_at,
    completed_at: v2.completed_at || null,
    error: normalizedStatus === 'failed'
      ? v2.total_chunks > 0
        ? `合成失败：分片进度 ${v2.done_chunks}/${v2.total_chunks}`
        : '合成失败'
      : null,
    progress: v2.total_chunks > 0 ? v2.done_chunks / v2.total_chunks : 0,
    token_count: 0,  // 后端 batch 任务摘要不返回 token 数
    char_count: v2.content ? v2.content.length : 0,  // 直接从文本计算
    elapsed_secs: Math.round((endTime - startTime) / 1000),
    has_audio: !!v2.output_path,
    total_chunks: v2.total_chunks,
    current_chunk: v2.done_chunks,
    group_id: v2.group_id || null,
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

  async forceTask(id: string): Promise<void> {
    await apiClient.post(`/api/v2/tasks/${id}/force`)
  },

  async cancelTask(id: string): Promise<void> {
    await apiClient.post(`/api/v2/tasks/${id}/cancel`)
  },

  async cancelAllTasks(): Promise<void> {
    await apiClient.post('/api/v2/tasks/cancel-all')
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

  // ── Providers ──────────────────────────────────────────────

  async listProviders(): Promise<ProviderInfo[]> {
    const response = await apiClient.get('/api/v2/providers')
    return response.data
  },

  async updateProviderKey(id: string, api_key: string): Promise<void> {
    await apiClient.put(`/api/v2/providers/${id}`, { api_key })
  },

  async setDefaultProvider(id: string): Promise<void> {
    await apiClient.put(`/api/v2/providers/${id}/default`)
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
