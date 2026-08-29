import { authedFetch, parseError, notifyUnauthorized } from '@/api/client'

/** Authenticated file download: fetch (Bearer) → Blob → <a download>.
 * Used for session ZIP export. */
export async function downloadViaFetch(subPath: string, filename: string): Promise<void> {
  const res = await authedFetch(subPath)
  if (res.status === 401) notifyUnauthorized()
  if (!res.ok) throw await parseError(res)
  const blob = await res.blob()
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  // Delayed revoke: let the browser start the download before releasing
  // the ObjectURL.
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}
