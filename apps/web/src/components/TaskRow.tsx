import { Link } from 'react-router'
import type { Task } from '@/api/endpoints'
import { TaskStatusBadge } from './StatusBadge'
import { ProgressBar } from './ProgressBar'
import { formatDateTime, formatDuration, taskProgress } from '@/lib/status'

// Virtual scrolling needs a fixed row height: h-[60px] here must match
// TaskListPage's estimateSize.
export const TASK_ROW_HEIGHT = 60

export function TaskRow({ task }: { task: Task }) {
  const progress = taskProgress(task.done_chunks, task.total_chunks)

  return (
    <Link
      to={`/tasks/${task.id}`}
      data-testid="task-row"
      className="grid h-[60px] grid-cols-[minmax(0,2fr)_auto_minmax(0,1fr)_minmax(0,1fr)_auto_auto] items-center gap-3 border-b border-border px-4 text-sm transition-colors hover:bg-surface-2"
    >
      <div className="min-w-0">
        <div className="truncate font-medium text-ink">{task.title}</div>
        <div className="num truncate text-xs text-ink-tertiary">{task.id}</div>
      </div>

      <span data-testid="task-row-status">
        <TaskStatusBadge status={task.status} />
      </span>

      <div className="flex min-w-0 flex-col gap-1">
        <div className="flex items-center gap-1.5">
          <span className="num text-xs text-ink-secondary">
            {task.done_chunks}/{task.total_chunks}
          </span>
          {task.failed_chunks > 0 ? (
            <span className="num text-xs text-red-500">失败 {task.failed_chunks}</span>
          ) : null}
        </div>
        <ProgressBar value={progress} className="max-w-[120px]" />
      </div>

      <div className="truncate text-xs text-ink-secondary">
        {task.voice} · {task.model}
      </div>

      <div className="num whitespace-nowrap text-xs text-ink-secondary">{formatDuration(task.duration_ms)}</div>

      <div className="num whitespace-nowrap text-xs text-ink-tertiary">{formatDateTime(task.created_at)}</div>
    </Link>
  )
}
