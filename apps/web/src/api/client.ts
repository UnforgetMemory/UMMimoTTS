// fetch wrapper. baseURL is empty — the vite dev proxy routes /api/v3 to the
// local Rust backend. On 401 the Bearer token from localStorage is used
// (first entered on the Settings page).

export const API_PREFIX = '/api/v3'
export const TOKEN_KEY = 'um-mimotts.token'

export class ApiError extends Error {
  readonly code: string
  readonly status: number

  constructor(code: string, message: string, status: number) {
    super(message)
    this.name = 'ApiError'
    this.code = code
    this.status = status
  }
}

export function getToken(): string | null {
  try {
    return localStorage.getItem(TOKEN_KEY)
  } catch {
    return null
  }
}

export function setToken(token: string): void {
  const v = token.trim()
  try {
    if (v) localStorage.setItem(TOKEN_KEY, v)
    else localStorage.removeItem(TOKEN_KEY)
  } catch {
    /* ignore */
  }
}

function notifyUnauthorized(): void {
  try {
    window.dispatchEvent(new CustomEvent('um-mimotts:unauthorized'))
  } catch {
    /* ignore */
  }
}

export { notifyUnauthorized }

export function authHeaders(extra?: HeadersInit): Headers {
  const headers = new Headers(extra)
  const token = getToken()
  if (token) headers.set('Authorization', `Bearer ${token}`)
  return headers
}

export async function authedFetch(path: string, init?: RequestInit): Promise<Response> {
  return fetch(`${API_PREFIX}${path}`, {
    ...init,
    headers: authHeaders(init?.headers),
  })
}

export async function parseError(res: Response): Promise<ApiError> {
  let code = 'INTERNAL'
  let message = `请求失败（HTTP ${res.status}）`
  try {
    const body: unknown = await res.json()
    if (body && typeof body === 'object') {
      const b = body as { code?: unknown; error?: unknown }
      if (typeof b.code === 'string') code = b.code
      if (typeof b.error === 'string') message = b.error
    }
  } catch {
    /* non-JSON error body */
  }
  return new ApiError(code, message, res.status)
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response
  try {
    res = await authedFetch(path, init)
  } catch (e) {
    throw new ApiError('NETWORK', e instanceof Error ? `网络错误：${e.message}` : '网络错误', 0)
  }
  if (res.status === 401) notifyUnauthorized()
  if (res.status === 204) return undefined as T
  if (!res.ok) throw await parseError(res)
  const text = await res.text()
  if (!text) return undefined as T
  try {
    return JSON.parse(text) as T
  } catch {
    throw new ApiError('BAD_RESPONSE', '响应 JSON 解析失败', res.status)
  }
}

export const api = {
  get: <T>(path: string) => request<T>(path),

  post: <T>(path: string, body?: unknown) =>
    request<T>(path, {
      method: 'POST',
      ...(body === undefined
        ? {}
        : { headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }),
    }),

  put: <T>(path: string, body?: unknown) =>
    request<T>(path, {
      method: 'PUT',
      ...(body === undefined
        ? {}
        : { headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }),
    }),

  del: <T>(path: string) => request<T>(path, { method: 'DELETE' }),

  postForm: <T>(path: string, form: FormData) => request<T>(path, { method: 'POST', body: form }),
}
