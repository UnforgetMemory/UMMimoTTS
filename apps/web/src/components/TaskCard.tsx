import { useEffect, useRef, useState } from 'react'
import { Link } from 'react-router'
import type { Task, TaskDetail } from '@/api/endpoints'
import { fetchTask } from '@/api/endpoints'
import { useEventSource } from '@/hooks/useEventSource'
import { TaskStatusBadge } from './StatusBadge'
import { ProgressBar } from './ProgressBar'
import { formatDuration, taskProgress } from '@/lib/status'

/**
 * Post-submit task progress card: subscribes to task:{id} SSE and debounces
 * a detail refetch. REST detail is authoritative — avoids double counting
 * after an SSE reconnect.
 */
export function TaskCard({ initialTask }: { initialTask: Task }) {
  const [detail, setDetail] = useState<TaskDetail | null>(null)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const task: Task = detail ?? initialTask
  const progress = taskProgress(task.done_chunks, task.total_chunks)

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [])

  const scheduleRefresh = () => {
    if (timerRef.current) clearTimeout(timerRef.current)
    timerRef.current = setTimeout(() => {
      fetchTask(initialTask.id)
        .then(setDetail)
        .catch(() => {
          /* backend not ready yet — ignore silently, wait for the next event */
        })
    }, 400)
  }

  // Terminal tasks never emit again: drop the channel to release the
  // EventSource instead of holding it for the card's lifetime.
  useEventSource({
    channel: ['done', 'failed', 'cancelled'].includes(task.status) ? null : `task:${initialTask.id}`,
    onEvent: scheduleRefresh,
  })

  return (
    <div className="rounded-xl border border-border bg-surface-2 p-4">
      <div className="flex items-center gap-2">
        <span className="min-w-0 flex-1 truncate text-sm font-medium text-ink">{task.title}</span>
        <TaskStatusBadge status={task.status} />
      </div>
      <div className="mt-2 flex items-center gap-2 text-xs text-ink-secondary">
        <span className="num">
          分片 {task.done_chunks}/{task.total_chunks}
        </span>
        {task.failed_chunks > 0 ? <span className="num text-red-500">失败 {task.failed_chunks}</span> : null}
        <span className="num ml-auto">{formatDuration(task.duration_ms)}</span>
      </div>
      <ProgressBar value={progress} className="mt-2" />
      {task.error ? <div className="mt-2 truncate text-xs text-red-500">{task.error}</div> : null}
      <div className="mt-3 flex items-center gap-3 text-xs">
        <Link to={`/tasks/${task.id}`} className="font-medium text-brand hover:text-brand-hover">
          查看详情 →
        </Link>
        {task.has_audio ? (
          <span className="text-green-600 dark:text-green-400">音频已就绪</span>
        ) : null}
      </div>
    </div>
  )
}
