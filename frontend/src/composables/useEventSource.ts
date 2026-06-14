import { ref, onMounted, onUnmounted } from 'vue'

export function useEventSource(url: string, onEvent: (data: any) => void) {
  const connected = ref(false)
  let es: EventSource | null = null
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null
  let attempt = 0

  function connect() {
    if (es) es.close()
    es = new EventSource(url)
    es.onopen = () => { connected.value = true; attempt = 0 }
    es.onmessage = (e) => {
      try { onEvent(JSON.parse(e.data)) } catch {}
    }
    es.onerror = () => {
      connected.value = false
      es?.close()
      const delay = Math.min(1000 * Math.pow(2, attempt), 30000)
      attempt++
      reconnectTimer = setTimeout(connect, delay)
    }
  }

  onMounted(connect)
  onUnmounted(() => {
    es?.close()
    if (reconnectTimer) clearTimeout(reconnectTimer)
  })

  return { connected }
}
