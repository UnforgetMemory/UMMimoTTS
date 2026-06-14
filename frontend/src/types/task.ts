export type TaskStatus = 'pending' | 'queued' | 'chunking' | 'processing' | 'merging' | 'done' | 'failed' | 'cancelled' | 'mergingfailed'

export interface Task {
  id: string
  custom_title?: string
  title?: string
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
  total_chunks?: number
  current_chunk?: number
}

export interface TaskListItem {
  id: string
  custom_title?: string
  title?: string
  status: TaskStatus
  voice: string | null
  model: string
  total_chars: number
  total_tokens: number
  total_chunks: number
  done_chunks: number
  failed_chunks: number
  has_audio: boolean
  created_at: string
  completed_at: string | null
  group_id?: string | null
}

export interface CreateTaskRequest {
  content: string
  voice: string
  model?: string
  context?: string
  title?: string
}
