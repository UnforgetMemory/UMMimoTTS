import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useAudioStore = defineStore('audio', () => {
  const isPlaying = ref(false)
  const currentUrl = ref<string | null>(null)
  const playbackRate = ref(1)
  const currentTime = ref(0)
  const duration = ref(0)
  const volume = ref(1)
  const isMuted = ref(false)

  let audio: HTMLAudioElement | null = null

  function getInstance(): HTMLAudioElement {
    if (!audio) {
      audio = new Audio()
      audio.preload = 'auto'
      audio.volume = volume.value

      audio.addEventListener('timeupdate', () => {
        currentTime.value = audio!.currentTime
      })
      audio.addEventListener('loadedmetadata', () => {
        duration.value = audio!.duration
        currentTime.value = 0
      })
      audio.addEventListener('ended', () => { isPlaying.value = false })
      audio.addEventListener('pause', () => {
        if (!audio!.seeking) isPlaying.value = false
      })
      audio.addEventListener('play', () => { isPlaying.value = true })
      audio.addEventListener('error', () => { isPlaying.value = false })
      audio.addEventListener('waiting', () => { /* buffering */ })
      audio.addEventListener('canplay', () => { /* ready */ })
    }
    return audio
  }

  /** Normalise a URL to an absolute href, returning null on failure. */
  function normalizeUrl(url: string): string | null {
    try {
      return new URL(url, window.location.origin).href
    } catch {
      return null
    }
  }

  /** Play after src changes — waits for `canplay` before calling play(). */
  function playAfterLoad(el: HTMLAudioElement) {
    el.playbackRate = playbackRate.value
    if (el.readyState >= 2) { // HAVE_CURRENT_DATA or better
      el.play().catch(() => {})
    } else {
      el.addEventListener('canplay', () => { el.play().catch(() => {}) }, { once: true })
    }
  }

  function play(url: string) {
    const el = getInstance()
    const normalized = normalizeUrl(url)
    if (!normalized) return
    if (el.src !== normalized) {
      el.src = normalized
      el.load()
      currentUrl.value = url
      currentTime.value = 0
      duration.value = 0
      playAfterLoad(el)
      return
    }
    el.playbackRate = playbackRate.value
    el.play().catch(() => {})
  }

  function pause() {
    if (audio) audio.pause()
  }

  function toggle(url?: string) {
    const el = getInstance()
    if (url) {
      const normalized = normalizeUrl(url)
      if (normalized && el.src !== normalized) {
        // Different source — switch seamlessly
        el.src = normalized
        el.load()
        currentUrl.value = url
        currentTime.value = 0
        duration.value = 0
        playAfterLoad(el)
        return
      }
    }
    if (el.paused || el.ended) {
      el.playbackRate = playbackRate.value
      el.play().catch(() => {})
    } else {
      el.pause()
    }
  }

  function seek(time: number) {
    if (audio) {
      audio.currentTime = time
      currentTime.value = time
    }
  }

  function changeSpeed(rate: number) {
    playbackRate.value = rate
    if (audio) audio.playbackRate = rate
  }

  function changeVolume(val: number) {
    volume.value = val
    if (audio) {
      audio.volume = val
      audio.muted = false
      isMuted.value = false
    }
  }

  function toggleMute() {
    isMuted.value = !isMuted.value
    if (audio) audio.muted = isMuted.value
  }

  function stop() {
    if (audio) {
      audio.pause()
      audio.currentTime = 0
    }
    isPlaying.value = false
    currentTime.value = 0
  }

  function destroy() {
    if (audio) {
      audio.pause()
      audio.src = ''
    }
    audio = null
    isPlaying.value = false
    currentUrl.value = null
    currentTime.value = 0
    duration.value = 0
  }

  return {
    isPlaying, currentUrl, playbackRate, currentTime, duration, volume, isMuted,
    play, pause, toggle, seek, changeSpeed, changeVolume, toggleMute, stop, destroy,
  }
})
