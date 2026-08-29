import { useEffect, useRef, useState } from 'react'
import type { ChangeEvent, DragEvent } from 'react'
import type { ImportResult, Session } from '@/api/endpoints'
import { fetchSession, importFiles, sessionExportSubPath } from '@/api/endpoints'
import { downloadViaFetch } from '@/lib/download'
import { useConfigStore } from '@/stores/config'
import { useEventSource } from '@/hooks/useEventSource'
import { SessionStatusBadge } from '@/components/StatusBadge'
import { ProgressBar } from '@/components/ProgressBar'
import { Button, Card, ErrorNotice, Label, Select, TextInput } from '@/components/ui'
import { FileIcon, DownloadIcon } from '@/components/Icons'
import { taskProgress } from '@/lib/status'

export default function ImportPage() {
  const loadConfig = useConfigStore((s) => s.load)
  const config = useConfigStore((s) => s.config)

  const [files, setFiles] = useState<File[]>([])
  const [sessionName, setSessionName] = useState('')
  const [voice, setVoice] = useState('')
  const [model, setModel] = useState('')
  const [style, setStyle] = useState('')

  const [importing, setImporting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<ImportResult | null>(null)
  const [session, setSession] = useState<Session | null>(null)
  const [dragging, setDragging] = useState(false)

  const refreshTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  // Session fetch sequence: a stale response from an older import must not
  // overwrite the session of the current one.
  const sessionSeqRef = useRef(0)

  useEffect(() => {
    void loadConfig()
  }, [loadConfig])

  useEffect(() => {
    if (!config) return
    if (!voice) setVoice(config.default_voice)
    if (!model) setModel(config.default_model)
  }, [config, voice, model])

  const addFiles = (list: FileList | null) => {
    if (!list) return
    const arr = Array.from(list).filter((f) => /\.txt$/i.test(f.name))
    setFiles((prev) => [...prev, ...arr])
  }

  const onDrop = (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault()
    setDragging(false)
    addFiles(e.dataTransfer.files)
  }

  const onInputChange = (e: ChangeEvent<HTMLInputElement>) => {
    addFiles(e.target.files)
    e.target.value = ''
  }

  // Session progress: SSE session:{id} → debounced detail refetch.
  useEventSource({
    channel: result ? `session:${result.session_id}` : null,
    onEvent: () => {
      if (!result) return
      const seq = sessionSeqRef.current
      if (refreshTimer.current) clearTimeout(refreshTimer.current)
      refreshTimer.current = setTimeout(() => {
        fetchSession(result.session_id)
          .then((d) => {
            // Guard against a stale session overwriting a newer import.
            if (seq === sessionSeqRef.current) setSession(d)
          })
          .catch(() => {
            /* transient error — wait for the next event */
          })
      }, 400)
    },
  })

  useEffect(() => {
    return () => {
      if (refreshTimer.current) clearTimeout(refreshTimer.current)
    }
  }, [])

  const submit = async () => {
    if (files.length === 0) {
      setError('请先选择要导入的 .txt 文件')
      return
    }
    const form = new FormData()
    for (const f of files) form.append('files', f)
    if (sessionName.trim()) form.append('session_name', sessionName.trim())
    if (voice) form.append('voice', voice)
    if (model) form.append('model', model)
    if (style.trim()) form.append('style', style.trim())

    setImporting(true)
    setError(null)
    try {
      const res = await importFiles(form)
      const seq = ++sessionSeqRef.current
      setResult(res)
      setSession(null)
      setFiles([])
      // Initial session detail pull.
      fetchSession(res.session_id)
        .then((d) => {
          if (seq === sessionSeqRef.current) setSession(d)
        })
        .catch(() => {})
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setImporting(false)
    }
  }

  const progress = session ? taskProgress(session.done_tasks, session.total_tasks) : 0

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <ErrorNotice message={error} />

      <Card>
        <h2 className="mb-1 text-base font-semibold text-ink">批量导入</h2>
        <p className="mb-3 text-xs text-ink-tertiary">
          支持多文件拖放 / 选择；编码支持 UTF-8 与 GB18030，服务端自动探测（.txt）
        </p>

        <div
          onDragOver={(e) => {
            e.preventDefault()
            setDragging(true)
          }}
          onDragLeave={() => setDragging(false)}
          onDrop={onDrop}
          className={`flex flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed px-4 py-10 text-center transition-colors ${
            dragging ? 'border-brand bg-brand-soft' : 'border-border bg-surface'
          }`}
        >
          <FileIcon className="h-8 w-8 text-ink-tertiary" />
          <div className="text-sm text-ink-secondary">拖入 .txt 文件到此处，或点击选择</div>
          <label className="cursor-pointer">
            <span className="inline-flex items-center rounded-lg bg-brand px-3 py-1.5 text-sm font-medium text-white transition-colors hover:bg-brand-hover">
              选择文件
            </span>
            <input type="file" multiple accept=".txt,text/plain" className="hidden" onChange={onInputChange} />
          </label>
        </div>

        {files.length > 0 ? (
          <div className="mt-3 max-h-40 overflow-y-auto rounded-lg border border-border">
            {files.map((f, i) => (
              <div key={`${f.name}-${i}`} className="flex items-center justify-between border-b border-border px-3 py-1.5 text-xs last:border-b-0">
                <span className="truncate text-ink">{f.name}</span>
                <span className="num shrink-0 text-ink-tertiary">{(f.size / 1024).toFixed(1)} KB</span>
              </div>
            ))}
          </div>
        ) : null}

        <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2">
          <div>
            <Label htmlFor="sessionName">会话名（可选，默认按时间生成）</Label>
            <TextInput
              id="sessionName"
              value={sessionName}
              onChange={(e) => setSessionName(e.target.value)}
              placeholder="例如：第 3 批小说章节"
            />
          </div>
          <div>
            <Label htmlFor="importVoice">默认音色</Label>
            <Select id="importVoice" value={voice} onChange={(e) => setVoice(e.target.value)} className="w-full">
              {(config?.voices ?? []).map((v) => (
                <option key={v.id} value={v.id}>
                  {v.name}
                </option>
              ))}
            </Select>
          </div>
          <div>
            <Label htmlFor="importModel">默认模型</Label>
            <Select id="importModel" value={model} onChange={(e) => setModel(e.target.value)} className="w-full">
              {(config?.models ?? []).map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name}
                </option>
              ))}
            </Select>
          </div>
          <div>
            <Label htmlFor="importStyle">默认风格指令</Label>
            <TextInput
              id="importStyle"
              value={style}
              onChange={(e) => setStyle(e.target.value)}
              placeholder="例如：平静的语气"
            />
          </div>
        </div>

        <div className="mt-4 flex items-center justify-between">
          <span className="num text-xs text-ink-tertiary">已选 {files.length} 个文件</span>
          <Button onClick={submit} disabled={importing || files.length === 0}>
            {importing ? '导入中…' : '开始导入'}
          </Button>
        </div>
      </Card>

      {result ? (
        <Card>
          <h3 className="text-sm font-semibold text-ink">导入结果</h3>
          <div className="mt-2 flex flex-wrap items-center gap-3 text-sm">
            <span className="num text-ink-secondary">收到 {result.files_received} 个文件</span>
            <span className="num text-ink-secondary">创建 {result.tasks_created} 个任务</span>
            {session ? <SessionStatusBadge status={session.status} /> : null}
          </div>

          {session ? (
            <div className="mt-3">
              <div className="mb-1 flex items-center justify-between text-xs text-ink-secondary">
                <span className="num">
                  完成 {session.done_tasks}/{session.total_tasks}
                  {session.failed_tasks > 0 ? <span className="text-red-500"> · 失败 {session.failed_tasks}</span> : null}
                </span>
                <span className="num">{Math.round(progress * 100)}%</span>
              </div>
              <ProgressBar value={progress} />
            </div>
          ) : null}

          {result.rejected && result.rejected.length > 0 ? (
            <div className="mt-3 text-xs text-ink-secondary">
              <div className="mb-1">未导入文件（{result.rejected.length}）：</div>
              <div className="max-h-24 overflow-y-auto rounded-lg border border-border p-2">
                {result.rejected.map((r, i) => (
                  <div key={i} className="truncate text-red-500">
                    {r}
                  </div>
                ))}
              </div>
            </div>
          ) : null}

          <div className="mt-4 flex items-center gap-3">
            <Button
              onClick={() => {
                downloadViaFetch(sessionExportSubPath(result.session_id), `session-${result.session_id}.zip`).catch(
                  (e: unknown) => setError(e instanceof Error ? e.message : String(e)),
                )
              }}
            >
              <DownloadIcon className="h-4 w-4" />
              导出 ZIP
            </Button>
            <span className="text-xs text-ink-tertiary">导出经鉴权下载（fetch + Bearer）。</span>
          </div>
        </Card>
      ) : null}
    </div>
  )
}
