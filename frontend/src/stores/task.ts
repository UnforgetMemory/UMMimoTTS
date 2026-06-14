import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { taskApi } from '@/api/tasks'
import type { Task } from '@/types/task'

export const useTaskStore = defineStore('task', () => {
  const tasks = ref<Task[]>([])
  const loading = ref(false)
  const refreshing = ref(false)
  const error = ref<string | null>(null)
  const totalCount = ref(0)
  const currentPage = ref(0)
  const pageSize = ref(50)

  const totalPages = computed(() => Math.ceil(totalCount.value / pageSize.value))

  async function fetchTasks(page = 0) {
    loading.value = page === 0
    refreshing.value = page > 0
    error.value = null
    try {
      const res = await taskApi.list(page, pageSize.value)
      tasks.value = res.data
      totalCount.value = res.total
      currentPage.value = res.page
    } catch (e: any) {
      error.value = e.message || '加载失败'
    } finally {
      loading.value = false
      refreshing.value = false
    }
  }

  async function createTask(content: string, voice: string, model?: string, context?: string, title?: string) {
    return taskApi.create({ content, voice, model, context, title })
  }

  async function deleteTask(id: string) {
    await taskApi.delete(id)
    tasks.value = tasks.value.filter((t: Task) => t.id !== id)
  }

  async function retryTask(id: string) {
    await taskApi.retry(id)
    const idx = tasks.value.findIndex((t: Task) => t.id === id)
    if (idx >= 0) tasks.value[idx].status = 'pending'
  }

  return { tasks, loading, refreshing, error, totalCount, currentPage, pageSize, totalPages, fetchTasks, createTask, deleteTask, retryTask }
})
