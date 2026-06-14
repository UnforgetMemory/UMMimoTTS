import apiClient from './client'
import type { Task, CreateTaskRequest, PaginatedResponse } from '@/types/task'

export const taskApi = {
  async list(page = 0, pageSize = 50, status?: string, search?: string) {
    const params: any = { page, page_size: pageSize }
    if (status) params.status = status
    if (search) params.search = search
    const { data } = await apiClient.get('/api/v2/tasks', { params })
    return data as PaginatedResponse<Task>
  },

  async get(id: string) {
    const { data } = await apiClient.get(`/api/v2/tasks/${id}`)
    return data as Task
  },

  async create(req: CreateTaskRequest) {
    const { data } = await apiClient.post('/api/v2/tasks', req)
    return data as { id: string; status: string }
  },

  async delete(id: string) {
    await apiClient.delete(`/api/v2/tasks/${id}`)
  },

  async retry(id: string) {
    const { data } = await apiClient.post(`/api/v2/tasks/${id}/retry`)
    return data
  },

  async updateTitle(id: string, title: string) {
    await apiClient.patch(`/api/v2/tasks/${id}/title`, { title })
  },

  getAudioUrl(id: string) {
    return `/api/v2/tasks/${id}/audio`
  },
}
