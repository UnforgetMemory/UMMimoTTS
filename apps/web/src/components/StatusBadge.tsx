import type { ReactNode } from 'react'
import type { ChunkStatus, SessionStatus, TaskStatus } from '@/api/endpoints'
import { CHUNK_STATUS_LABELS, SESSION_STATUS_LABELS, TASK_STATUS_LABELS } from '@/lib/status'

const FALLBACK_STYLE = 'bg-surface-3 text-ink-secondary'
const FALLBACK_LABEL = '未知'

const TASK_STYLES: Record<TaskStatus, string> = {
  pending: 'bg-surface-3 text-ink-secondary',
  queued: 'bg-blue-500/10 text-blue-600 dark:text-blue-400',
  synthesizing: 'bg-brand-soft text-brand',
  merging: 'bg-amber-500/10 text-amber-600 dark:text-amber-400',
  done: 'bg-green-500/10 text-green-600 dark:text-green-400',
  failed: 'bg-red-500/10 text-red-600 dark:text-red-400',
  cancelled: 'bg-surface-3 text-ink-tertiary',
}

const SESSION_STYLES: Record<SessionStatus, string> = {
  active: 'bg-blue-500/10 text-blue-600 dark:text-blue-400',
  completed: 'bg-green-500/10 text-green-600 dark:text-green-400',
  failed: 'bg-red-500/10 text-red-600 dark:text-red-400',
  cancelled: 'bg-surface-3 text-ink-tertiary',
}

const CHUNK_STYLES: Record<ChunkStatus, string> = {
  pending: 'bg-surface-3 text-ink-secondary',
  inflight: 'bg-brand-soft text-brand',
  done: 'bg-green-500/10 text-green-600 dark:text-green-400',
  failed: 'bg-red-500/10 text-red-600 dark:text-red-400',
}

function Badge({ className, children }: { className: string; children: ReactNode }) {
  return (
    <span className={`inline-flex shrink-0 items-center rounded-md px-2 py-0.5 text-xs font-medium ${className}`}>
      {children}
    </span>
  )
}

export function TaskStatusBadge({ status }: { status: TaskStatus }) {
  // Runtime defense: unknown backend status falls back to the neutral style
  // and a placeholder label instead of rendering undefined.
  return <Badge className={TASK_STYLES[status] ?? FALLBACK_STYLE}>{TASK_STATUS_LABELS[status] ?? FALLBACK_LABEL}</Badge>
}

export function SessionStatusBadge({ status }: { status: SessionStatus }) {
  return <Badge className={SESSION_STYLES[status] ?? FALLBACK_STYLE}>{SESSION_STATUS_LABELS[status] ?? FALLBACK_LABEL}</Badge>
}

export function ChunkStatusBadge({ status }: { status: ChunkStatus }) {
  return <Badge className={CHUNK_STYLES[status] ?? FALLBACK_STYLE}>{CHUNK_STATUS_LABELS[status] ?? FALLBACK_LABEL}</Badge>
}
