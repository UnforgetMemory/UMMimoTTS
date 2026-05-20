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

export type TaskStatus = 'pending' | 'queued' | 'synthesizing' | 'streaming' | 'completed' | 'failed'

export interface Task {
  id: string
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
  api_key?: string
}

export interface TaskEvent {
  task_id: string
  event_type: 'status_changed' | 'completed' | 'failed'
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
    
    eventSource.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        onEvent(data)
      } catch (error) {
        console.error('Failed to parse SSE event:', error)
      }
    }
    
    eventSource.onerror = () => {
      console.error('SSE connection error for task:', taskId)
      eventSource.close()
    }
    
    return eventSource
  },

  // 获取音色试听 URL
  getVoicePreviewUrl(voiceId: string): string {
    return `/api/v1/voices/${voiceId}/preview`
  },
}
