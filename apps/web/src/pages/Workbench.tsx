import { useEffect, useRef, useState } from 'react'
import type { DragEvent } from 'react'
import type { CreateTaskRequest, Task } from '@/api/endpoints'
import { createTask, voicePreviewUrl } from '@/api/endpoints'
import { useConfigStore } from '@/stores/config'
import { extractTitle } from '@/lib/status'
import { VoiceCard } from '@/components/VoiceCard'
import { TaskCard } from '@/components/TaskCard'
import { Button, Card, ErrorNotice, Label, Select, TextArea, TextInput } from '@/components/ui'

const TAGS = ['[笑]', '[吸气]', '[语速加快]', '(唱歌)']

export default function Workbench() {
  const loadConfig = useConfigStore((s) => s.load)
  const config = useConfigStore((s) => s.config)
  const configError = useConfigStore((s) => s.error)

  const [content, setContent] = useState('')
  const [style, setStyle] = useState('')
  const [voice, setVoice] = useState('')
  const [model, setModel] = useState('')
  const [providerId, setProviderId] = useState('')

  const [submitting, setSubmitting] = useState(false)
  const [submitError, setSubmitError] = useState<string | null>(null)
  const [recent, setRecent] = useState<Task[]>([])

  const [dragging, setDragging] = useState(false)
  const [playingVoice, setPlayingVoice] = useState<string | null>(null)

  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const audioRef = useRef<HTMLAudioElement | null>(null)
  // Preview request sequence: a slow stale response must never replace the
  // audio element of the voice the user actually selected last.
  const previewSeqRef = useRef(0)

  useEffect(() => {
    void loadConfig()
  }, [loadConfig])

  // Populate default voice/model/provider once config arrives.
  useEffect(() => {
    if (!config) return
    if (!voice) setVoice(config.default_voice)
    if (!model) setModel(config.default_model)
    if (!providerId) {
      const def = config.providers.find((p) => p.is_default)
      setProviderId(def?.id ?? '')
    }
  }, [config, voice, model, providerId])

  const insertAtCursor = (text: string) => {
    const el = textareaRef.current
    if (!el) {
      setContent((c) => c + text)
      return
    }
    const start = el.selectionStart
    const end = el.selectionEnd
    const next = content.slice(0, start) + text + content.slice(end)
    setContent(next)
    requestAnimationFrame(() => {
      el.focus()
      const pos = start + text.length
      el.setSelectionRange(pos, pos)
    })
  }

  const onDrop = async (e: DragEvent<HTMLTextAreaElement>) => {
    e.preventDefault()
    setDragging(false)
    const file = e.dataTransfer.files?.[0]
    if (!file) return
    if (!/\.txt$/i.test(file.name) && file.type !== 'text/plain') {
      setSubmitError('仅支持 .txt 文本文件')
      return
    }
    try {
      const text = await file.text()
      const el = textareaRef.current
      if (el) {
        const start = el.selectionStart
        const end = el.selectionEnd
        const next = content.slice(0, start) + text + content.slice(end)
        setContent(next)
      } else {
        setContent((c) => (c ? `${c}\n${text}` : text))
      }
    } catch {
      setSubmitError('读取文件失败')
    }
  }

  const togglePreview = async (id: string) => {
    if (playingVoice === id) {
      audioRef.current?.pause()
      setPlayingVoice(null)
      return
    }
    audioRef.current?.pause()
    audioRef.current = null
    setPlayingVoice(id)
    const seq = ++previewSeqRef.current
    try {
      // Previews go through the /voices/{id}/preview proxy (scoped token;
      // backend 302 → CDN allowlist).
      const url = await voicePreviewUrl(id)
      if (seq !== previewSeqRef.current) return // a newer toggle won the race
      const audio = new Audio(url)
      audio.onended = () => setPlayingVoice(null)
      audio.onerror = () => setPlayingVoice(null)
      audioRef.current = audio
      void audio.play()
    } catch {
      if (seq === previewSeqRef.current) setPlayingVoice(null)
    }
  }

  useEffect(() => {
    return () => {
      audioRef.current?.pause()
      audioRef.current = null
    }
  }, [])

  const submit = async () => {
    if (!content.trim()) {
      setSubmitError('请输入待合成文本')
      return
    }
    if (!voice) {
      setSubmitError('请选择音色')
      return
    }
    const req: CreateTaskRequest = {
      title: extractTitle(content),
      content,
      voice,
      model,
      priority: 0,
      style: style.trim() ? style.trim() : null,
      provider_id: providerId || null,
    }
    setSubmitting(true)
    setSubmitError(null)
    try {
      const task = await createTask(req)
      setRecent((list) => [task, ...list])
      setContent('')
    } catch (e) {
      setSubmitError(e instanceof Error ? e.message : String(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      {configError ? <ErrorNotice message={`加载配置失败：${configError}`} /> : null}
      <ErrorNotice message={submitError} />

      <Card>
        <div className="mb-2 flex items-center justify-between">
          <h2 className="text-base font-semibold text-ink">合成工作台</h2>
          <span className="text-xs text-ink-tertiary">支持拖入 .txt（UTF-8）</span>
        </div>

        <Label htmlFor="content">待合成文本</Label>
        <TextArea
          id="content"
          data-testid="workbench-content"
          ref={textareaRef}
          rows={8}
          value={content}
          onChange={(e) => setContent(e.target.value)}
          onDragOver={(e) => {
            e.preventDefault()
            setDragging(true)
          }}
          onDragLeave={() => setDragging(false)}
          onDrop={onDrop}
          placeholder="输入或拖入要合成的文本……"
          className={dragging ? 'ring-2 ring-brand-ring' : ''}
        />

        <div className="mt-3 flex flex-wrap items-center gap-2">
          <span className="text-xs text-ink-secondary">音频标签：</span>
          {TAGS.map((tag) => (
            <button
              key={tag}
              type="button"
              onClick={() => insertAtCursor(tag)}
              className="rounded-md border border-border bg-surface px-2 py-1 text-xs text-ink-secondary transition-colors hover:border-brand/50 hover:text-brand"
            >
              {tag}
            </button>
          ))}
          <span className="ml-auto text-xs text-ink-tertiary">点击插入光标处</span>
        </div>

        <div className="mt-4">
          <Label htmlFor="style">风格指令（user 消息，不会被朗读）</Label>
          <TextInput
            id="style"
            value={style}
            onChange={(e) => setStyle(e.target.value)}
            placeholder="例如：用温柔的语气，语速稍慢"
          />
        </div>
      </Card>

      <Card>
        <h3 className="mb-3 text-sm font-semibold text-ink">音色（{config?.voices.length ?? 0}）</h3>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-3">
          {(config?.voices ?? []).map((v) => (
            <VoiceCard
              key={v.id}
              voice={v}
              selected={voice === v.id}
              onSelect={setVoice}
              playing={playingVoice === v.id}
              onTogglePlay={togglePreview}
            />
          ))}
        </div>
      </Card>

      <Card className="flex flex-col gap-4 sm:flex-row sm:items-end">
        <div className="flex-1">
          <Label htmlFor="model">模型</Label>
          <Select id="model" value={model} onChange={(e) => setModel(e.target.value)} className="w-full">
            {(config?.models ?? []).map((m) => (
              <option key={m.id} value={m.id}>
                {m.name}
              </option>
            ))}
          </Select>
        </div>
        <div className="flex-1">
          <Label htmlFor="provider">Provider</Label>
          <Select id="provider" value={providerId} onChange={(e) => setProviderId(e.target.value)} className="w-full">
            <option value="">自动（默认供应商）</option>
            {(config?.providers ?? []).map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
                {p.is_default ? '（默认）' : ''}
              </option>
            ))}
          </Select>
        </div>
        <Button onClick={submit} disabled={submitting || !config} data-testid="workbench-submit">
          {submitting ? '提交中…' : '开始合成'}
        </Button>
      </Card>

      {recent.length > 0 ? (
        <div className="space-y-3">
          <h3 className="text-sm font-semibold text-ink">最近任务</h3>
          {recent.map((t) => (
            <TaskCard key={t.id} initialTask={t} />
          ))}
        </div>
      ) : null}
    </div>
  )
}
