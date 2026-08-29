// Status maps / formatters — pure functions covered by vitest units.
import type { components } from '@/api/v3'

export type TaskStatus = components['schemas']['TaskStatus']
export type SessionStatus = components['schemas']['SessionStatus']
export type ChunkStatus = components['schemas']['ChunkStatus']

export const TASK_STATUS_LABELS: Record<TaskStatus, string> = {
  pending: '待处理',
  queued: '排队中',
  synthesizing: '合成中',
  merging: '合并中',
  done: '已完成',
  failed: '失败',
  cancelled: '已取消',
}

export const SESSION_STATUS_LABELS: Record<SessionStatus, string> = {
  active: '进行中',
  completed: '已完成',
  failed: '失败',
  cancelled: '已取消',
}

export const CHUNK_STATUS_LABELS: Record<ChunkStatus, string> = {
  pending: '待合成',
  inflight: '合成中',
  done: '已完成',
  failed: '失败',
}

const TERMINAL_TASK_STATUSES: ReadonlySet<TaskStatus> = new Set(['done', 'failed', 'cancelled'])

export function isTerminalTaskStatus(status: TaskStatus): boolean {
  return TERMINAL_TASK_STATUSES.has(status)
}

export function isActiveTaskStatus(status: TaskStatus): boolean {
  return !isTerminalTaskStatus(status)
}

/** Task progress ratio clamped to [0,1]; total<=0 → 0. */
export function taskProgress(done: number, total: number): number {
  if (total <= 0) return 0
  return Math.min(1, Math.max(0, done / total))
}

/** Duration formatting: ms → human-readable Chinese units. */
export function formatDuration(ms: number | null | undefined): string {
  if (ms == null || Number.isNaN(ms) || ms < 0) return '—'
  if (ms < 1000) return `${Math.round(ms)} 毫秒`
  // round to whole seconds first so 59950ms reads "60.0 秒", not "60.0"
  const totalSec = Math.round(ms / 1000)
  if (totalSec < 60) return `${(ms / 1000).toFixed(1)} 秒`
  const minutes = Math.floor(totalSec / 60)
  const seconds = totalSec % 60
  if (minutes < 60) return seconds > 0 ? `${minutes} 分 ${seconds} 秒` : `${minutes} 分钟`
  const hours = Math.floor(minutes / 60)
  const restMin = minutes % 60
  return restMin > 0 ? `${hours} 小时 ${restMin} 分` : `${hours} 小时`
}

/** ISO timestamp → local `YYYY-MM-DD HH:mm:ss`. */
export function formatDateTime(iso: string | null | undefined): string {
  if (!iso) return '—'
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return '—'
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
}

/** Breaker cooldown countdown formatting (seconds). */
export function formatCountdown(secs: number): string {
  const total = Math.ceil(secs)
  if (total <= 0) return '0 秒'
  if (total < 60) return `${total} 秒`
  const minutes = Math.floor(total / 60)
  const seconds = total % 60
  if (minutes < 60) return seconds > 0 ? `${minutes} 分 ${seconds} 秒` : `${minutes} 分钟`
  const hours = Math.floor(minutes / 60)
  const restMin = minutes % 60
  return restMin > 0 ? `${hours} 小时 ${restMin} 分` : `${hours} 小时`
}

/** Title from content: first non-empty line, `(唱歌)` prefix stripped,
 * truncated to max chars. */
export function extractTitle(content: string, max = 60): string {
  const lines = content.split(/\r?\n/)
  for (const raw of lines) {
    const line = raw.trim()
    if (line) {
      const cleaned = line.replace(/^\(\s*唱歌\s*\)/, '').trim()
      const text = cleaned || '未命名任务'
      return text.length > max ? `${text.slice(0, max)}…` : text
    }
  }
  return '未命名任务'
}
