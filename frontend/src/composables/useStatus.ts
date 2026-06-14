import type { TaskStatus, GroupStatus } from '@/api/client'
import type { BadgeVariants } from '@/components/ui/badge'

// ── Task Status ──────────────────────────────────────────────

export function getTaskStatusText(status: TaskStatus): string {
  const map: Record<TaskStatus, string> = {
    pending: '等待中',
    queued: '排队中',
    chunking: '分片中',
    processing: '合成中',
    merging: '合并中',
    mergingfailed: '合并失败',
    paused: '已暂停',
    done: '已完成',
    failed: '失败',
    cancelled: '已取消',
  }
  return map[status] || status
}

export function getTaskStatusVariant(status: TaskStatus): BadgeVariants['variant'] {
  const map: Record<TaskStatus, BadgeVariants['variant']> = {
    pending: 'warning',
    queued: 'warning',
    chunking: 'default',
    processing: 'default',
    merging: 'default',
    mergingfailed: 'destructive',
    paused: 'outline',
    done: 'success',
    failed: 'destructive',
    cancelled: 'secondary',
  }
  return map[status] || 'secondary'
}

/** Lighter variant — uses string instead of TaskStatus, suitable for dynamic lookup */
export function getTaskStatusVariantRaw(status: string): BadgeVariants['variant'] {
  switch (status) {
    case 'completed':
    case 'done': return 'success'
    case 'failed':
    case 'mergingfailed': return 'destructive'
    case 'processing':
    case 'chunking':
    case 'merging': return 'default'
    case 'queued':
    case 'pending': return 'warning'
    case 'paused': return 'outline'
    case 'cancelled': return 'secondary'
    default: return 'secondary'
  }
}

export function getTaskStatusLabelRaw(status: string): string {
  switch (status) {
    case 'pending': return '等待'
    case 'queued': return '队列中'
    case 'chunking': return '分片中'
    case 'processing': return '合成中'
    case 'merging': return '合并中'
    case 'mergingfailed': return '合并失败'
    case 'completed':
    case 'done': return '完成'
    case 'failed': return '失败'
    case 'paused': return '暂停'
    case 'cancelled': return '取消'
    default: return status
  }
}

// ── Group Status ─────────────────────────────────────────────

export function getGroupStatusLabel(status: GroupStatus): string {
  switch (status) {
    case 'pending': return '等待中'
    case 'queued': return '队列中'
    case 'preparing': return '准备中'
    case 'processing': return '处理中'
    case 'paused': return '已暂停'
    case 'completed': return '已完成'
    case 'failed': return '失败'
    case 'cancelled': return '已取消'
    default: return status
  }
}

export function getGroupStatusVariant(status: GroupStatus): BadgeVariants['variant'] {
  switch (status) {
    case 'pending':
    case 'queued':
    case 'preparing': return 'secondary'
    case 'processing': return 'default'
    case 'paused': return 'outline'
    case 'completed': return 'success'
    case 'failed': return 'destructive'
    case 'cancelled': return 'secondary'
    default: return 'secondary'
  }
}

// ── Formatting ───────────────────────────────────────────────

export function formatShortDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    })
  } catch {
    return iso
  }
}

/** Full local datetime: YYYY/MM/DD HH:mm:ss */
export function formatLocalDateTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    })
  } catch {
    return iso
  }
}

export function formatFullTimestamp(iso: string | null): string {
  if (!iso) return '—'
  try {
    return new Date(iso).toLocaleString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    })
  } catch {
    return iso
  }
}

export function formatElapsed(secs: number | null): string {
  if (secs == null) return '—'
  if (secs < 60) return `${Math.floor(secs)}s`
  if (secs < 3600) return `${Math.floor(secs / 60)}m ${Math.floor(secs % 60)}s`
  return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`
}

export function truncateId(id: string, maxLen = 8): string {
  return id.length > maxLen ? id.slice(0, maxLen) + '…' : id
}

export function formatTokens(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(1)}M`
  if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(1)}K`
  return tokens.toLocaleString()
}

export function getTaskProgress(status: string): number {
  switch (status) {
    case 'queued': return 10
    case 'chunking': return 30
    case 'processing': return 60
    case 'merging': return 90
    default: return 0
  }
}

export function isActiveStatus(status: string): boolean {
  return ['queued', 'chunking', 'processing', 'merging'].includes(status)
}
