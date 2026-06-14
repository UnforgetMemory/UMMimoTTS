import { ref, onUnmounted } from 'vue'

export function useAudio() {
  const currentUrl = ref<string | null>(null)
  const playing = ref(false)
  const currentTime = ref(0)
  const duration = ref(0)
  const volume = ref(1)
  const playbackRate = ref(1)
  let audio: HTMLAudioElement | null = null

  function play(url: string) {
    if (currentUrl.value === url && playing.value) {
      audio?.pause()
      playing.value = false
      return
    }
    if (audio) { audio.pause(); audio = null }
    currentUrl.value = url
    audio = new Audio(url)
    audio.volume = volume.value
    audio.playbackRate = playbackRate.value
    audio.onplay = () => { playing.value = true }
    audio.onpause = () => { playing.value = false }
    audio.ontimeupdate = () => { currentTime.value = audio?.currentTime || 0 }
    audio.onloadedmetadata = () => { duration.value = audio?.duration || 0 }
    audio.onended = () => { playing.value = false; currentTime.value = 0 }
    audio.play()
  }

  function pause() { audio?.pause(); playing.value = false }
  function stop() { if (audio) { audio.pause(); audio = null }; playing.value = false; currentUrl.value = null }
  function seek(time: number) { if (audio) audio.currentTime = time }
  function setVolume(v: number) { volume.value = v; if (audio) audio.volume = v }
  function setRate(r: number) { playbackRate.value = r; if (audio) audio.playbackRate = r }

  onUnmounted(() => { if (audio) { audio.pause(); audio = null } })

  return { currentUrl, playing, currentTime, duration, volume, playbackRate, play, pause, stop, seek, setVolume, setRate }
}
