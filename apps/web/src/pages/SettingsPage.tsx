import { useEffect, useState } from 'react'
import type { Provider } from '@/api/endpoints'
import { fetchProviders, saveProviderKey, setDefaultProvider, updateProvider } from '@/api/endpoints'
import { useAuthStore } from '@/stores/auth'
import { useConfigStore } from '@/stores/config'
import { useStatsStore } from '@/stores/stats'
import { queueDepth, workerCount } from '@/lib/stats'
import { Button, Card, ErrorNotice, TextInput } from '@/components/ui'

const ENDPOINTS: { method: string; path: string; desc: string }[] = [
  { method: 'GET', path: '/api/v3/health', desc: '健康检查（免鉴权）' },
  { method: 'GET', path: '/api/v3/config', desc: '音色/模型/供应商/默认值/公告' },
  { method: 'GET', path: '/api/v3/providers', desc: '供应商列表（api_key 永不下发）' },
  { method: 'PUT', path: '/api/v3/providers/{id}/key', desc: '保存供应商 API Key' },
  { method: 'PUT', path: '/api/v3/providers/{id}/default', desc: '设为默认供应商' },
  { method: 'GET/POST', path: '/api/v3/sessions', desc: '导入会话列表 / 创建' },
  { method: 'GET/DELETE', path: '/api/v3/sessions/{id}', desc: '会话详情 / 删除' },
  { method: 'POST', path: '/api/v3/sessions/{id}/cancel', desc: '取消会话内非终态任务' },
  { method: 'GET', path: '/api/v3/sessions/{id}/export', desc: '导出会话 ZIP' },
  { method: 'POST', path: '/api/v3/import', desc: '批量导入 TXT（multipart）' },
  { method: 'GET/POST', path: '/api/v3/tasks', desc: '任务列表 / 创建' },
  { method: 'GET/DELETE', path: '/api/v3/tasks/{id}', desc: '任务详情 / 删除' },
  { method: 'POST', path: '/api/v3/tasks/{id}/retry', desc: '重试失败任务' },
  { method: 'POST', path: '/api/v3/tasks/{id}/cancel', desc: '取消任务' },
  { method: 'GET', path: '/api/v3/tasks/{id}/audio', desc: '任务音频（Range 流式）' },
  { method: 'GET', path: '/api/v3/tasks/{id}/download', desc: '单任务音频下载' },
  { method: 'GET', path: '/api/v3/events', desc: 'SSE 事件流（channel + token query）' },
  { method: 'GET', path: '/api/v3/stats', desc: '引擎运行时统计（队列/worker/熔断）' },
]

export default function SettingsPage() {
  const token = useAuthStore((s) => s.token)
  const setToken = useAuthStore((s) => s.setToken)
  const clearToken = useAuthStore((s) => s.clearToken)
  const config = useConfigStore((s) => s.config)

  const [tokenInput, setTokenInput] = useState('')
  const [tokenMsg, setTokenMsg] = useState<string | null>(null)

  const [providers, setProviders] = useState<Provider[]>([])
  const [keyInputs, setKeyInputs] = useState<Record<string, string>>({})
  const [providersError, setProvidersError] = useState<string | null>(null)
  const [actionMsg, setActionMsg] = useState<string | null>(null)

  const [editingId, setEditingId] = useState<string | null>(null)
  const [editForm, setEditForm] = useState({ name: '', base_url: '', budget_group: '' })

  const stats = useStatsStore((s) => s.stats)
  const statsError = useStatsStore((s) => s.error)
  const refreshStats = useStatsStore((s) => s.refresh)

  useEffect(() => {
    fetchProviders()
      .then(setProviders)
      .catch((e: unknown) => setProvidersError(e instanceof Error ? e.message : String(e)))
  }, [])

  // The Shell already polls /stats every 5s; refresh once on mount so the
  // page has data immediately.
  useEffect(() => {
    void refreshStats()
  }, [refreshStats])

  const saveKey = async (p: Provider) => {
    const key = keyInputs[p.id] ?? ''
    if (!key.trim()) {
      setActionMsg('请输入 API Key')
      return
    }
    setActionMsg(null)
    try {
      await saveProviderKey(p.id, key.trim())
      setActionMsg(`已保存 ${p.name} 的 API Key（服务端密文落盘）`)
      setKeyInputs((prev) => ({ ...prev, [p.id]: '' }))
      setProviders(await fetchProviders())
    } catch (e) {
      setActionMsg(e instanceof Error ? e.message : String(e))
    }
  }

  const makeDefault = async (p: Provider) => {
    setActionMsg(null)
    try {
      await setDefaultProvider(p.id)
      setActionMsg(`已将 ${p.name} 设为默认供应商`)
      setProviders(await fetchProviders())
    } catch (e) {
      setActionMsg(e instanceof Error ? e.message : String(e))
    }
  }

  const startEdit = (p: Provider) => {
    setEditingId(p.id)
    setEditForm({ name: p.name, base_url: p.base_url, budget_group: p.budget_group ?? '' })
    setActionMsg(null)
  }

  const cancelEdit = () => {
    setEditingId(null)
    setEditForm({ name: '', base_url: '', budget_group: '' })
  }

  const saveEdit = async (p: Provider) => {
    setActionMsg(null)
    try {
      await updateProvider(p.id, {
        name: editForm.name.trim() || p.name,
        base_url: editForm.base_url.trim() || p.base_url,
        budget_group: editForm.budget_group.trim() || undefined,
      })
      setActionMsg(`已更新 ${p.name} 的配置`)
      cancelEdit()
      setProviders(await fetchProviders())
    } catch (e) {
      setActionMsg(e instanceof Error ? e.message : String(e))
    }
  }

  const saveToken = () => {
    setToken(tokenInput)
    setTokenMsg(tokenInput.trim() ? 'API Token 已保存到本地' : 'API Token 已清除')
    setTokenInput('')
  }

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <ErrorNotice message={providersError ?? actionMsg} />

      <Card>
        <h2 className="mb-1 text-base font-semibold text-ink">API Token（本地鉴权）</h2>
        <p className="mb-3 text-xs text-ink-tertiary">
          后端 REST 需要 Bearer 鉴权；Token 保存在浏览器 localStorage，每次请求经 Authorization 头携带。
        </p>
        <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
          <TextInput
            type="password"
            data-testid="settings-token-input"
            value={tokenInput}
            onChange={(e) => setTokenInput(e.target.value)}
            placeholder={token ? '已配置（留空并保存可清除）' : '输入 API Token'}
          />
          <div className="flex shrink-0 gap-2">
            <Button onClick={saveToken}>保存 Token</Button>
            {token ? <Button variant="ghost" onClick={clearToken}>清除</Button> : null}
          </div>
        </div>
        <div className="mt-2 text-xs text-ink-tertiary">
          {tokenMsg ?? (token ? `当前状态：已配置 ${'•'.repeat(8)}` : '当前状态：未配置')}
        </div>
      </Card>

      <Card>
        <h2 className="mb-3 text-base font-semibold text-ink">供应商（Provider）</h2>
        {providers.length === 0 ? (
          <div className="text-sm text-ink-tertiary">暂无供应商配置</div>
        ) : (
          <div className="space-y-3">
            {providers.map((p) => (
              <div key={p.id} className="rounded-lg border border-border bg-surface p-3">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-sm font-medium text-ink">{p.name}</span>
                  <span className="rounded bg-surface-3 px-1.5 py-0.5 text-xs text-ink-secondary">{p.kind}</span>
                  {p.is_configured ? (
                    <span className="rounded bg-green-500/10 px-1.5 py-0.5 text-xs text-green-600 dark:text-green-400">已配置</span>
                  ) : (
                    <span className="rounded bg-amber-500/10 px-1.5 py-0.5 text-xs text-amber-600 dark:text-amber-400">未配置</span>
                  )}
                  {p.is_default ? (
                    <span className="rounded bg-brand-soft px-1.5 py-0.5 text-xs text-brand">默认</span>
                  ) : (
                    <Button variant="ghost" className="px-2 py-0.5 text-xs" onClick={() => makeDefault(p)}>
                      设为默认
                    </Button>
                  )}
                  <Button variant="ghost" className="px-2 py-0.5 text-xs" onClick={() => startEdit(p)}>
                    编辑
                  </Button>
                </div>
                <div className="num mt-1 text-xs text-ink-tertiary">{p.base_url}</div>
                {p.budget_group ? <div className="text-xs text-ink-tertiary">预算组：{p.budget_group}</div> : null}
                <div className="mt-2 flex gap-2">
                  <TextInput
                    type="password"
                    value={keyInputs[p.id] ?? ''}
                    onChange={(e) => setKeyInputs((prev) => ({ ...prev, [p.id]: e.target.value }))}
                    placeholder={p.is_configured ? '输入新 Key 覆盖保存' : '输入 API Key'}
                  />
                  <Button variant="outline" onClick={() => saveKey(p)}>
                    保存 Key
                  </Button>
                </div>

                {editingId === p.id ? (
                  <div className="mt-3 space-y-2 rounded-lg border border-brand/30 bg-surface-2 p-3">
                    <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
                      <div>
                        <div className="mb-1 text-xs text-ink-tertiary">名称</div>
                        <TextInput
                          value={editForm.name}
                          onChange={(e) => setEditForm((f) => ({ ...f, name: e.target.value }))}
                          placeholder="供应商名称"
                        />
                      </div>
                      <div>
                        <div className="mb-1 text-xs text-ink-tertiary">base_url</div>
                        <TextInput
                          value={editForm.base_url}
                          onChange={(e) => setEditForm((f) => ({ ...f, base_url: e.target.value }))}
                          placeholder="https://…"
                        />
                      </div>
                      <div>
                        <div className="mb-1 text-xs text-ink-tertiary">预算组</div>
                        <TextInput
                          value={editForm.budget_group}
                          onChange={(e) => setEditForm((f) => ({ ...f, budget_group: e.target.value }))}
                          placeholder="default"
                        />
                      </div>
                    </div>
                    <div className="flex justify-end gap-2">
                      <Button variant="ghost" onClick={cancelEdit}>
                        取消
                      </Button>
                      <Button onClick={() => saveEdit(p)}>保存修改</Button>
                    </div>
                  </div>
                ) : null}
              </div>
            ))}
          </div>
        )}
      </Card>

      <Card>
        <h2 className="mb-3 text-base font-semibold text-ink">服务端统计（/stats）</h2>
        {stats ? (
          <>
            <div className="num grid grid-cols-2 gap-2 sm:grid-cols-2">
              <div className="rounded-lg border border-border bg-surface p-3">
                <div className="text-xs text-ink-tertiary">队列深度</div>
                <div className="mt-1 text-lg font-semibold text-ink">{queueDepth(stats)}</div>
              </div>
              <div className="rounded-lg border border-border bg-surface p-3">
                <div className="text-xs text-ink-tertiary">Worker 数</div>
                <div className="mt-1 text-lg font-semibold text-ink">{workerCount(stats)}</div>
              </div>
            </div>
            <div className="mt-3">
              <div className="mb-1 text-xs text-ink-tertiary">Provider 运行时（AIMD 窗口 / 熔断）</div>
              {stats.providers && stats.providers.length > 0 ? (
                <div className="scrollbar-thin overflow-x-auto rounded-lg border border-border">
                  <table className="w-full text-left text-xs">
                    <thead className="bg-surface-2 text-ink-tertiary">
                      <tr>
                        <th className="px-3 py-1.5 font-medium">Provider</th>
                        <th className="px-3 py-1.5 font-medium">预算组</th>
                        <th className="px-3 py-1.5 font-medium">窗口</th>
                        <th className="px-3 py-1.5 font-medium">进行中</th>
                        <th className="px-3 py-1.5 font-medium">熔断</th>
                        <th className="px-3 py-1.5 font-medium">冷却</th>
                      </tr>
                    </thead>
                    <tbody>
                      {(stats.providers ?? []).map((p) => (
                        <tr key={p.provider_id ?? '?'} className="border-t border-border">
                          <td className="num px-3 py-1.5 text-ink">{p.provider_id ?? '—'}</td>
                          <td className="num px-3 py-1.5 text-ink-secondary">{p.group ?? '—'}</td>
                          <td className="num px-3 py-1.5 text-ink-secondary">{p.window ?? '—'}</td>
                          <td className="num px-3 py-1.5 text-ink-secondary">{p.inflight ?? '—'}</td>
                          <td className="px-3 py-1.5">
                            {p.open ? (
                              <span className="rounded bg-red-500/10 px-1.5 py-0.5 text-red-600 dark:text-red-400">打开</span>
                            ) : (
                              <span className="rounded bg-green-500/10 px-1.5 py-0.5 text-green-600 dark:text-green-400">关闭</span>
                            )}
                          </td>
                          <td className="num px-3 py-1.5 text-ink-secondary">{p.retry_after_secs ?? '—'}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              ) : (
                <div className="text-xs text-ink-tertiary">无 provider 数据</div>
              )}
            </div>
          </>
        ) : (
          <div className="text-sm text-ink-tertiary">{statsError ? `统计加载失败：${statsError}` : '统计加载中…'}</div>
        )}
      </Card>

      {config?.chunk ? (
        <Card>
          <h2 className="mb-3 text-base font-semibold text-ink">分片设置（服务端下发）</h2>
          <div className="num grid grid-cols-3 gap-2">
            <div className="rounded-lg border border-border bg-surface p-3">
              <div className="text-xs text-ink-tertiary">上下文窗口</div>
              <div className="mt-1 text-sm font-semibold text-ink">{config.chunk.context_window_tokens ?? '—'}</div>
            </div>
            <div className="rounded-lg border border-border bg-surface p-3">
              <div className="text-xs text-ink-tertiary">目标 Token</div>
              <div className="mt-1 text-sm font-semibold text-ink">{config.chunk.target_tokens ?? '—'}</div>
            </div>
            <div className="rounded-lg border border-border bg-surface p-3">
              <div className="text-xs text-ink-tertiary">硬上限 Token</div>
              <div className="mt-1 text-sm font-semibold text-ink">{config.chunk.hard_cap_tokens ?? '—'}</div>
            </div>
          </div>
        </Card>
      ) : null}

      <Card>
        <h2 className="mb-3 text-base font-semibold text-ink">API 端点参考（对照 OpenAPI 契约）</h2>
        <div className="scrollbar-thin max-h-80 overflow-y-auto rounded-lg border border-border">
          {ENDPOINTS.map((e) => (
            <div key={`${e.method}-${e.path}`} className="flex items-center gap-3 border-b border-border px-3 py-1.5 text-xs last:border-b-0">
              <span className="num w-24 shrink-0 font-medium text-brand">{e.method}</span>
              <span className="num min-w-0 flex-1 truncate text-ink-secondary">{e.path}</span>
              <span className="shrink-0 text-ink-tertiary">{e.desc}</span>
            </div>
          ))}
        </div>
      </Card>

      <div className="pb-4 text-center text-xs text-ink-tertiary">
        UM-MimoTTS v4 工作台 · 契约先行（ADR-003）· 默认深色主题
      </div>
    </div>
  )
}
