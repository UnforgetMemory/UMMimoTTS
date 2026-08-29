// Typed API endpoints. Every business type derives from the generated
// contract file v3.d.ts (single source of truth: packages/contract/openapi.yaml).
// Hand-written business interfaces are forbidden.
import type { components, paths } from './v3'
import { api, API_PREFIX } from './client'
import { scopedUrl } from './scoped'

export type Task = components['schemas']['Task']
export type TaskDetail = components['schemas']['TaskDetail']
export type TaskPage = components['schemas']['TaskPage']
export type TaskStatus = components['schemas']['TaskStatus']
export type Chunk = components['schemas']['Chunk']
export type ChunkStatus = components['schemas']['ChunkStatus']
export type Session = components['schemas']['Session']
export type SessionPage = components['schemas']['SessionPage']
export type SessionStatus = components['schemas']['SessionStatus']
export type Config = components['schemas']['Config']
export type Provider = components['schemas']['Provider']
export type VoicePreset = components['schemas']['VoicePreset']
export type ModelPreset = components['schemas']['ModelPreset']
export type CreateTaskRequest = components['schemas']['CreateTaskRequest']
export type ImportResult = components['schemas']['ImportResult']

// —— /stats (engine runtime stats; inline schema derived from paths) ——
export type ServerStats = paths['/stats']['get']['responses']['200']['content']['application/json']
export type ProviderStat = NonNullable<ServerStats['providers']>[number]

// —— /providers/{id} (metadata edit; inline body derived from paths) ——
export type UpdateProviderRequest = paths['/providers/{id}']['put']['requestBody']['content']['application/json']

// —— /auth/scoped (inline body/response derived from paths) ——
export type ScopedTokenResponse = paths['/auth/scoped']['post']['responses']['200']['content']['application/json']

// —— query params derived from contract paths (ADR-003: zero hand-written
// business types) ——
export type TaskListParams = paths['/tasks']['get']['parameters']['query']
export type SessionListParams = paths['/sessions']['get']['parameters']['query']

function qs(params: Record<string, string | number | undefined>): string {
  const sp = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== '') sp.set(k, String(v))
  }
  const s = sp.toString()
  return s ? `?${s}` : ''
}

const enc = encodeURIComponent

// —— /config ——
export function fetchConfig(): Promise<Config> {
  return api.get('/config')
}

// —— /providers ——
export function fetchProviders(): Promise<Provider[]> {
  return api.get('/providers')
}

export function saveProviderKey(id: string, apiKey: string): Promise<void> {
  return api.put(`/providers/${enc(id)}/key`, { api_key: apiKey })
}

export function setDefaultProvider(id: string): Promise<void> {
  return api.put(`/providers/${enc(id)}/default`)
}

export function updateProvider(id: string, body: UpdateProviderRequest): Promise<void> {
  return api.put(`/providers/${enc(id)}`, body)
}

// —— /sessions ——
export function fetchSessions(params: SessionListParams = {}): Promise<SessionPage> {
  return api.get(`/sessions${qs({ page: params.page, page_size: params.page_size })}`)
}

export function createSession(name: string): Promise<Session> {
  return api.post('/sessions', { name })
}

export function fetchSession(id: string): Promise<Session> {
  return api.get(`/sessions/${enc(id)}`)
}

export function deleteSession(id: string): Promise<void> {
  return api.del(`/sessions/${enc(id)}`)
}

export function cancelSession(id: string): Promise<void> {
  return api.post(`/sessions/${enc(id)}/cancel`)
}

// —— /tasks ——
export function fetchTasks(params: TaskListParams = {}): Promise<TaskPage> {
  return api.get(
    `/tasks${qs({
      page: params.page,
      page_size: params.page_size,
      status: params.status,
      session_id: params.session_id,
      search: params.search,
    })}`,
  )
}

export function createTask(req: CreateTaskRequest): Promise<Task> {
  return api.post('/tasks', req)
}

export function fetchTask(id: string): Promise<TaskDetail> {
  return api.get(`/tasks/${enc(id)}`)
}

export function deleteTask(id: string): Promise<void> {
  return api.del(`/tasks/${enc(id)}`)
}

export function retryTask(id: string): Promise<void> {
  return api.post(`/tasks/${enc(id)}/retry`)
}

export function cancelTask(id: string): Promise<void> {
  return api.post(`/tasks/${enc(id)}/cancel`)
}

// —— /import ——
export function importFiles(form: FormData): Promise<ImportResult> {
  return api.postForm('/import', form)
}

// —— /stats ——
export function fetchStats(): Promise<ServerStats> {
  return api.get('/stats')
}

// —— audio / preview / download: URL contexts use scoped tokens
// (audio:{id} / preview:{id}) instead of the raw API token ——
export function taskAudioUrl(id: string): Promise<string> {
  return scopedUrl(`${API_PREFIX}/tasks/${enc(id)}/audio`, `audio:${id}`)
}

export function taskDownloadUrl(id: string): Promise<string> {
  return scopedUrl(`${API_PREFIX}/tasks/${enc(id)}/download`, `audio:${id}`)
}

export function voicePreviewUrl(id: string): Promise<string> {
  return scopedUrl(`${API_PREFIX}/voices/${enc(id)}/preview`, `preview:${id}`)
}

// ZIP export (/sessions/{id}/export declares no token query in the contract,
// so it goes through fetch → Blob; see lib/download.ts).
export const sessionExportSubPath = (id: string) => `/sessions/${enc(id)}/export`
