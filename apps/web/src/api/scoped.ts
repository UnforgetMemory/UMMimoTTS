// Scoped credentials (option B): URL-only contexts (audio/events/preview)
// exchange the raw API token for a short-lived HMAC-signed credential.
// getScopedToken calls POST /auth/scoped (Bearer), caches per scope with a
// TTL of expires_in − 30s leeway.
import type { paths } from './v3'
import { api, ApiError } from './client'

type ScopedResponse = paths['/auth/scoped']['post']['responses']['200']['content']['application/json']

const LEEWAY_MS = 30_000

interface Entry {
  token: string
  expiresAt: number
}

const cache = new Map<string, Entry>()
const inflight = new Map<string, Promise<string>>()
// Bumped on clearScopedCache: late in-flight responses must not repopulate
// the cache after the credentials were invalidated.
let generation = 0

export function clearScopedCache(): void {
  generation += 1
  cache.clear()
}

async function requestScopedToken(scope: string): Promise<ScopedResponse> {
  return api.post<ScopedResponse>('/auth/scoped', { scope })
}

/** Fetch a short-lived credential for the scope; cached within TTL and
 * deduplicated per scope while in flight. 401 clears the cache and throws. */
export async function getScopedToken(scope: string): Promise<string> {
  const hit = cache.get(scope)
  if (hit && hit.expiresAt > Date.now()) return hit.token

  const pending = inflight.get(scope)
  if (pending) return pending

  // Captured BEFORE the await: a clearScopedCache() while the request is in
  // flight must prevent this stale (old-token) credential from repopulating
  // the cache afterwards.
  const gen = generation
  const p = (async () => {
    try {
      const res = await requestScopedToken(scope)
      const ttlMs = res.expires_in * 1000
      // Very short TTLs get half their lifetime instead of expiring on the
      // spot (leeway would clamp to zero → refetch on every call).
      const expiresAt =
        Date.now() + (ttlMs > LEEWAY_MS * 2 ? ttlMs - LEEWAY_MS : Math.floor(ttlMs / 2))
      if (gen === generation) cache.set(scope, { token: res.token, expiresAt })
      return res.token
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) clearScopedCache()
      throw e
    }
  })().finally(() => {
    inflight.delete(scope)
  })

  inflight.set(scope, p)
  return p
}

/** Append a scoped `?token=` to a GET path whose context cannot send an
 * Authorization header (EventSource / media elements). */
export async function scopedUrl(path: string, scope: string): Promise<string> {
  const token = await getScopedToken(scope)
  return `${path}?token=${encodeURIComponent(token)}`
}
