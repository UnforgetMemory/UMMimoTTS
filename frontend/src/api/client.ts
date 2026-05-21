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
  created_at: string
  completed_at: string | null
  error: string | null
  progress: number
  token_count: number
  char_count: number
  elapsed_secs: number | null
  has_audio: boolean
}

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

export const api = {
  async getVoices(): Promise<Voice[]> {
    const response = await apiClient.get('/api/v1/voices')
    return response.data.voices
  },

  async synthesize(request: SynthesizeRequest): Promise<{ task_id: string }> {
    const response = await apiClient.post('/api/v1/tts/synthesize', request)
    return response.data
  },

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
}
