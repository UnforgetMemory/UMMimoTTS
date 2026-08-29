import { useEffect, useState } from 'react'
import { taskAudioUrl } from '@/api/endpoints'
import { useAuthStore } from '@/stores/auth'

export interface UseAudioUrlResult {
  url: string | null
}

/**
 * Task audio playback URL: resolves a scoped token (audio:{id}) and returns
 * `/api/v3/tasks/{id}/audio?token=<scoped>` for a native <audio> element.
 * The browser sends Range headers itself for streaming seek (backend 206),
 * so the whole file is never downloaded at once.
 */
export function useAudioUrl(taskId: string | undefined, hasAudio: boolean): UseAudioUrlResult {
  const token = useAuthStore((s) => s.token)
  const [url, setUrl] = useState<string | null>(null)

  useEffect(() => {
    if (!taskId || !hasAudio) {
      setUrl(null)
      return
    }
    let disposed = false
    setUrl(null)
    taskAudioUrl(taskId)
      .then((u) => {
        if (!disposed) setUrl(u)
      })
      .catch(() => {
        /* 401/network error: keep null; retried when the token next changes */
      })
    return () => {
      disposed = true
    }
  }, [taskId, hasAudio, token])

  return { url }
}
