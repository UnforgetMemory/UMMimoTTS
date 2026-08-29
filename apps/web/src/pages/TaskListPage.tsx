import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import type { DomainEvent } from '@/api/events'
import type { Session, Task, TaskStatus } from '@/api/endpoints'
import { fetchSessions, fetchTasks } from '@/api/endpoints'
import { useTaskStream } from '@/hooks/useTaskStream'
import { isActiveTaskStatus } from '@/lib/status'
import { TASK_ROW_HEIGHT, TaskRow } from '@/components/TaskRow'
import { Button, EmptyState, ErrorNotice, Select, TextInput } from '@/components/ui'
import { SearchIcon } from '@/components/Icons'

const PAGE_SIZE = 100
const STATUS_OPTIONS: { value: '' | TaskStatus; label: string }[] = [
  { value: '', label: '全部状态' },
  { value: 'pending', label: '待处理' },
  { value: 'queued', label: '排队中' },
  { value: 'synthesizing', label: '合成中' },
  { value: 'merging', label: '合并中' },
  { value: 'done', label: '已完成' },
  { value: 'failed', label: '失败' },
  { value: 'cancelled', label: '已取消' },
]

const GRID = 'grid grid-cols-[minmax(0,2fr)_auto_minmax(0,1fr)_minmax(0,1fr)_auto_auto] items-center gap-3 px-4'

export default function TaskListPage() {
  const [rows, setRows] = useState<Task[]>([])
  const [total, setTotal] = useState(0)
  const [page, setPage] = useState(0)
  const [hasMore, setHasMore] = useState(false)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const [status, setStatus] = useState<'' | TaskStatus>('')
  const [search, setSearch] = useState('')
  const [searchInput, setSearchInput] = useState('')
  const [sessionId, setSessionId] = useState('')
  const [sessions, setSessions] = useState<Session[]>([])

  const parentRef = useRef<HTMLDivElement>(null)
  const loadingRef = useRef(false)
  const requestSeq = useRef(0)

  // Session dropdown data (for the session filter).
  useEffect(() => {
    fetchSessions({ page: 0, page_size: 100 })
      .then((p) => setSessions(p.data))
      .catch(() => {})
  }, [])

  const loadPage = useCallback(
    async (targetPage: number, replace: boolean) => {
      if (loadingRef.current && !replace) return
      const seq = ++requestSeq.current
      loadingRef.current = true
      setLoading(true)
      setError(null)
      try {
        const res = await fetchTasks({
          page: targetPage,
          page_size: PAGE_SIZE,
          status: status || undefined,
          session_id: sessionId || undefined,
          search: search || undefined,
        })
        if (seq !== requestSeq.current) return // filter changed: stale response
        setRows((prev) => (replace ? res.data : [...prev, ...res.data]))
        setTotal(res.total)
        setPage(targetPage)
        setHasMore((targetPage + 1) * PAGE_SIZE < res.total)
      } catch (e) {
        if (seq !== requestSeq.current) return
        setError(e instanceof Error ? e.message : String(e))
      } finally {
        if (seq === requestSeq.current) {
          loadingRef.current = false
          setLoading(false)
        }
      }
    },
    [status, search, sessionId],
  )

  // Filter changes reset to page 0.
  useEffect(() => {
    void loadPage(0, true)
  }, [loadPage])

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => TASK_ROW_HEIGHT,
    overscan: 12,
  })

  // Near-bottom scroll loads the next page.
  const lastIndex = virtualizer.getVirtualItems().at(-1)?.index ?? -1
  useEffect(() => {
    if (hasMore && !loading && lastIndex >= rows.length - 10) {
      void loadPage(page + 1, false)
    }
  }, [hasMore, loading, lastIndex, rows.length, page, loadPage])

  // SSE live updates: subscribe non-terminal task channels; task:{id}
  // events patch the matching row.
  const inflightIds = useMemo(() => rows.filter((t) => isActiveTaskStatus(t.status)).map((t) => t.id), [rows])

  const onTaskEvent = useCallback((e: DomainEvent) => {
    if (e.type === 'provider_health' || e.type === 'session_updated') return
    const taskId = e.task_id
    setRows((prev) =>
      prev.map((t) => {
        if (t.id !== taskId) return t
        switch (e.type) {
          case 'task_status_changed':
            return { ...t, status: e.status as TaskStatus }
          case 'task_completed':
            return { ...t, status: 'done', done_chunks: t.total_chunks, duration_ms: e.duration_ms }
          case 'task_failed':
            return { ...t, status: 'failed', error: e.error }
          case 'chunk_completed':
            return { ...t, done_chunks: Math.min(t.total_chunks, t.done_chunks + 1) }
          case 'chunk_failed':
            return { ...t, failed_chunks: Math.min(t.total_chunks, t.failed_chunks + 1) }
          case 'all_chunks_done':
            return { ...t, done_chunks: t.total_chunks }
          default:
            return t
        }
      }),
    )
  }, [])

  useTaskStream({ taskIds: inflightIds, onEvent: onTaskEvent })

  return (
    <div className="flex h-full flex-col">
      {/* filter bar */}
      <div className="flex flex-wrap items-center gap-2 border-b border-border px-4 py-3">
        <Select value={status} onChange={(e) => setStatus(e.target.value as '' | TaskStatus)} className="w-32">
          {STATUS_OPTIONS.map((o) => (
            <option key={o.value || 'all'} value={o.value}>
              {o.label}
            </option>
          ))}
        </Select>

        <Select value={sessionId} onChange={(e) => setSessionId(e.target.value)} className="w-44">
          <option value="">全部会话</option>
          {sessions.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </Select>

        <div className="relative">
          <SearchIcon className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-tertiary" />
          <TextInput
            value={searchInput}
            onChange={(e) => setSearchInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') setSearch(searchInput.trim())
            }}
            placeholder="搜索标题 / 正文…"
            className="w-64 pl-8"
          />
        </div>
        <Button
          variant="outline"
          onClick={() => {
            setSearch(searchInput.trim())
          }}
        >
          搜索
        </Button>

        <span className="num ml-auto text-xs text-ink-tertiary">共 {total} 条</span>
      </div>

      <ErrorNotice message={error} className="m-3 mb-0" />

      {/* header row */}
      <div className={`${GRID} border-b border-border bg-surface-2 py-2 text-xs font-medium text-ink-tertiary`}>
        <span>任务</span>
        <span>状态</span>
        <span>进度</span>
        <span>音色 · 模型</span>
        <span>时长</span>
        <span>创建时间</span>
      </div>

      {/* virtual scrolling list */}
      <div ref={parentRef} className="scrollbar-thin flex-1 overflow-y-auto">
        {rows.length === 0 && !loading ? (
          <EmptyState title="暂无任务" hint="前往工作台创建合成任务，或批量导入 TXT" />
        ) : (
          <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
            {virtualizer.getVirtualItems().map((vi) => {
              const task = rows[vi.index]
              return (
                <div
                  key={task.id}
                  style={{
                    position: 'absolute',
                    top: 0,
                    left: 0,
                    width: '100%',
                    transform: `translateY(${vi.start}px)`,
                  }}
                >
                  <TaskRow task={task} />
                </div>
              )
            })}
          </div>
        )}
        {loading ? (
          <div className="py-4 text-center text-xs text-ink-tertiary">加载中…</div>
        ) : null}
        {!hasMore && rows.length > 0 ? (
          <div className="py-4 text-center text-xs text-ink-tertiary">已全部加载</div>
        ) : null}
      </div>
    </div>
  )
}
