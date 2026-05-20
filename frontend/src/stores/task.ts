import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { api, type Task, type TaskStatus, type TaskEvent } from '@/api/client'
import { useIntervalFn } from '@vueuse/core'

export const useTaskStore = defineStore('task', () => {
  const tasks = ref<Task[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  
  // SSE 连接管理
  const eventSources = new Map<string, EventSource>()

  // 计算属性
  const completedTasks = computed(() => tasks.value.filter(t => t.status === 'completed'))
  const failedTasks = computed(() => tasks.value.filter(t => t.status === 'failed'))
  const pendingTasks = computed(() => tasks.value.filter(t => 
    ['pending', 'queued', 'synthesizing', 'streaming'].includes(t.status)
  ))

  // 加载任务列表
  async function loadTasks() {
    loading.value = true
    error.value = null
    try {
      const data = await api.getTasks()
      tasks.value = data.sort((a, b) => 
        new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
      )
    } catch (err: any) {
      error.value = err.message || '加载任务失败'
      console.error('Failed to load tasks:', err)
    } finally {
      loading.value = false
    }
  }

  // 创建任务
  async function createTask(request: {
    text: string
    voice: string
    model: string
    context?: string
    api_key?: string
  }) {
    loading.value = true
    error.value = null
    try {
      const result = await api.synthesize(request)
      await loadTasks()
      
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

  // 删除任务
  async function removeTask(taskId: string) {
    try {
      await api.deleteTask(taskId)
      tasks.value = tasks.value.filter(t => t.id !== taskId)
    } catch (err: any) {
      error.value = err.message || '删除任务失败'
      console.error('Failed to delete task:', err)
      throw err
    }
  }

  // 更新单个任务状态
  function updateTaskStatus(taskId: string, updates: Partial<Task>) {
    const task = tasks.value.find(t => t.id === taskId)
    if (task) {
      Object.assign(task, updates)
    }
  }

  // 订阅任务 SSE 事件
  function subscribeToTaskEvents(taskId: string) {
    // 如果已经订阅过，先关闭旧的连接
    if (eventSources.has(taskId)) {
      eventSources.get(taskId)?.close()
    }
    
    const eventSource = api.subscribeToTask(taskId, (event: TaskEvent) => {
      console.log('SSE Event received:', event)
      
      switch (event.event_type) {
        case 'status_changed':
          updateTaskStatus(taskId, { 
            progress: event.progress ?? 0 
          })
          break
        case 'completed':
          updateTaskStatus(taskId, { 
            status: 'completed',
            progress: 1.0 
          })
          // 完成后关闭连接
          eventSources.delete(taskId)
          eventSource.close()
          // 刷新任务列表以获取最新数据
          loadTasks()
          break
        case 'failed':
          updateTaskStatus(taskId, { 
            status: 'failed',
            error: event.error || '未知错误'
          })
          // 失败后关闭连接
          eventSources.delete(taskId)
          eventSource.close()
          loadTasks()
          break
      }
    })
    
    eventSources.set(taskId, eventSource)
  }

  // 清理所有 SSE 连接
  function cleanup() {
    eventSources.forEach(es => es.close())
    eventSources.clear()
  }

  // 自动刷新（5秒间隔）
  const { pause: pauseRefresh, resume: resumeRefresh } = useIntervalFn(
    () => loadTasks(),
    5000,
    { immediate: false }
  )

  // 初始化时加载任务并开始自动刷新
  function init() {
    loadTasks()
    resumeRefresh()
  }

  return {
    tasks,
    loading,
    error,
    completedTasks,
    failedTasks,
    pendingTasks,
    loadTasks,
    createTask,
    removeTask,
    updateTaskStatus,
    pauseRefresh,
    resumeRefresh,
    init,
    cleanup,
  }
})
