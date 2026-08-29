import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router'
import type { TaskDetail } from '@/api/endpoints'
import { cancelTask, deleteTask, fetchTask, retryTask, taskDownloadUrl } from '@/api/endpoints'
import { useEventSource } from '@/hooks/useEventSource'
import { useAudioUrl } from '@/hooks/useAudioUrl'
import { useAuthStore } from '@/stores/auth'
import { ChunkStatusBadge, TaskStatusBadge } from '@/components/StatusBadge'
import { ProgressBar } from '@/components/ProgressBar'
import { Button, Card, ErrorNotice, Spinner } from '@/components/ui'
import { ChevronLeftIcon, DownloadIcon, RefreshIcon, TrashIcon, XIcon } from '@/components/Icons'
import { formatDateTime, formatDuration, taskProgress } from '@/lib/status'

export default function TaskDetailPage() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const token = useAuthStore((s) => s.token)

  const [detail, setDetail] = useState<TaskDetail | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [downloadUrl, setDownloadUrl] = useState<string | null>(null)
  const refreshTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const requestSeq = useRef(0)

  const load = useCallback(async () => {
    if (!id) return
    const seq = ++requestSeq.current
    setLoading(true)
    setError(null)
    try {
      const data = await fetchTask(id)
      if (seq !== requestSeq.current) return // route reuse: drop stale response
      setDetail(data)
    } catch (e) {
      if (seq !== requestSeq.current) return
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      if (seq === requestSeq.current) setLoading(false)
    }
  }, [id])

  useEffect(() => {
    void load()
    return () => {
      if (refreshTimer.current) clearTimeout(refreshTimer.current)
    }
  }, [load])

  // Download link: resolve the scoped token (audio:{id}) asynchronously.
  useEffect(() => {
    if (!id || !detail?.has_audio) {
      setDownloadUrl(null)
      return
    }
    let disposed = false
    setDownloadUrl(null)
    taskDownloadUrl(id)
      .then((u) => {
        if (!disposed) setDownloadUrl(u)
      })
      .catch(() => {})
    return () => {
      disposed = true
    }
  }, [id, detail?.has_audio, token])

  // SSE live refresh: task:{id} events → debounced detail refetch (the
  // authoritative state), stale responses discarded.
  useEventSource({
    channel: id ? `task:${id}` : null,
    onEvent: () => {
      if (refreshTimer.current) clearTimeout(refreshTimer.current)
      const seq = requestSeq.current
      refreshTimer.current = setTimeout(() => {
        fetchTask(id as string)
          .then((d) => {
            if (seq === requestSeq.current) setDetail(d)
          })
          .catch(() => {})
      }, 400)
    },
  })

  const audio = useAudioUrl(id, Boolean(detail?.has_audio))

  // fn may return true to skip the post-action reload (delete navigates away
  // and must not refetch the removed task onto an unmounting component).
  const runAction = async (name: string, fn: () => Promise<void | boolean>) => {
    setBusy(name)
    setActionError(null)
    try {
      const skipReload = await fn()
      if (!skipReload) await load()
    } catch (e) {
      setActionError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(null)
    }
  }

  if (loading && !detail) {
    return (
      <div className="flex h-full items-center justify-center text-ink-tertiary">
        <Spinner className="h-6 w-6" />
      </div>
    )
  }

  if (!detail) {
    return (
      <div className="p-6">
        <ErrorNotice message={error ?? '任务不存在'} />
        <Link to="/tasks" className="mt-3 inline-block text-sm text-brand hover:text-brand-hover">
          ← 返回任务历史
        </Link>
      </div>
    )
  }

  const progress = taskProgress(detail.done_chunks, detail.total_chunks)
  const chunks = detail.chunks ?? []

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <div className="flex items-center gap-2">
        <Link
          to="/tasks"
          className="inline-flex items-center gap-1 text-sm text-ink-secondary transition-colors hover:text-ink"
        >
          <ChevronLeftIcon className="h-4 w-4" />
          任务历史
        </Link>
      </div>

      <ErrorNotice message={error ?? actionError} />

      <Card>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="truncate text-lg font-semibold text-ink">{detail.title}</h1>
            <div className="num mt-1 text-xs text-ink-tertiary">{detail.id}</div>
            <div className="mt-1 flex flex-wrap gap-2 text-xs text-ink-secondary">
              <span>音色：{detail.voice}</span>
              <span>模型：{detail.model}</span>
              {detail.style ? <span>风格：{detail.style}</span> : null}
              {detail.session_id ? <span className="num">会话：{detail.session_id}</span> : null}
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <TaskStatusBadge status={detail.status} />
          </div>
        </div>

        <div className="mt-4">
          <div className="mb-1 flex items-center justify-between text-xs text-ink-secondary">
            <span className="num">
              分片 {detail.done_chunks}/{detail.total_chunks}
              {detail.failed_chunks > 0 ? <span className="text-red-500"> · 失败 {detail.failed_chunks}</span> : null}
            </span>
            <span className="num">{formatDuration(detail.duration_ms)}</span>
          </div>
          <ProgressBar value={progress} />
        </div>

        <div className="num mt-3 flex flex-wrap gap-4 text-xs text-ink-tertiary">
          <span>字符 {detail.total_chars ?? '—'}</span>
          <span>Token {detail.total_tokens ?? '—'}</span>
          <span>创建 {formatDateTime(detail.created_at)}</span>
          <span>完成 {formatDateTime(detail.completed_at)}</span>
        </div>

        {detail.error ? <div className="mt-3 rounded-lg bg-red-500/10 px-3 py-2 text-xs text-red-500">{detail.error}</div> : null}

        <div className="mt-4 flex flex-wrap items-center gap-2">
          {detail.has_audio ? (
            <>
              {audio.url ? (
                <audio controls src={audio.url} data-testid="task-detail-audio" className="h-9 w-full max-w-xl" />
              ) : null}
              {downloadUrl ? (
                <a
                  href={downloadUrl}
                  download={`${detail.title || detail.id}.wav`}
                  className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-border px-3 py-1.5 text-sm font-medium text-ink transition-colors hover:bg-surface-2"
                >
                  <DownloadIcon className="h-4 w-4" />
                  下载
                </a>
              ) : null}
            </>
          ) : (
            <span className="text-xs text-ink-tertiary">音频尚未就绪</span>
          )}

          <div className="ml-auto flex gap-2">
            {detail.status === 'failed' ? (
              <Button variant="outline" disabled={busy !== null} onClick={() => runAction('retry', () => retryTask(detail.id))}>
                <RefreshIcon className="h-4 w-4" />
                {busy === 'retry' ? '重试中…' : '重试'}
              </Button>
            ) : null}
            {!['done', 'failed', 'cancelled'].includes(detail.status) ? (
              <Button variant="outline" disabled={busy !== null} onClick={() => runAction('cancel', () => cancelTask(detail.id))}>
                <XIcon className="h-4 w-4" />
                取消
              </Button>
            ) : null}
            <Button
              variant="danger"
              disabled={busy !== null}
              onClick={() =>
                runAction('delete', async () => {
                  await deleteTask(detail.id)
                  // Invalidate in-flight fetches/timers before unmounting.
                  requestSeq.current += 1
                  navigate('/tasks')
                  return true
                })
              }
            >
              <TrashIcon className="h-4 w-4" />
              删除
            </Button>
          </div>
        </div>
      </Card>

      <Card>
        <h2 className="mb-3 text-sm font-semibold text-ink">文本预览</h2>
        <pre className="scrollbar-thin max-h-64 overflow-y-auto whitespace-pre-wrap rounded-lg border border-border bg-surface p-3 text-sm leading-relaxed text-ink">
          {detail.content ?? '（无内容）'}
        </pre>
      </Card>

      <Card>
        <h2 className="mb-3 text-sm font-semibold text-ink">分片列表（{chunks.length}）</h2>
        {chunks.length === 0 ? (
          <div className="text-xs text-ink-tertiary">暂无分片</div>
        ) : (
          <div className="divide-y divide-border">
            {chunks.map((c) => (
              <div key={c.id} className="flex items-center gap-3 py-2">
                <span className="num w-8 shrink-0 text-xs text-ink-tertiary">#{c.seq}</span>
                <ChunkStatusBadge status={c.status} />
                <span className="num text-xs text-ink-secondary">重试 {c.retry_count}</span>
                <span className="num text-xs text-ink-secondary">{formatDuration(c.duration_ms)}</span>
                <div className="min-w-0 flex-1 truncate text-xs text-ink-tertiary" title={c.text}>
                  {c.text ?? ''}
                </div>
                {c.error ? <div className="max-w-[40%] truncate text-xs text-red-500" title={c.error}>{c.error}</div> : null}
              </div>
            ))}
          </div>
        )}
      </Card>
    </div>
  )
}
