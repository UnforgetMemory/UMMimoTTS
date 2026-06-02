<template>
  <Dialog :open="open" @update:open="$emit('update:open', $event)">
    <DialogContent class="sm:max-w-3xl max-h-[90vh] overflow-hidden flex flex-col">
      <DialogHeader class="shrink-0">
        <DialogTitle>音频播放器</DialogTitle>
        <DialogDescription class="sr-only">TTS 音频播放器</DialogDescription>
      </DialogHeader>

      <!-- Fixed Audio Player Section -->
      <div class="shrink-0 space-y-4 p-4 pb-3 border-b bg-card">
        <!-- Progress Bar (native range, reliable seeking) -->
        <div class="space-y-1.5" :class="{ 'opacity-40 pointer-events-none': !audioReady }">
          <input
            type="range"
            min="0"
            :max="duration || 0"
            step="0.01"
            :value="currentTime"
            @pointerdown="onSeekStart"
            @input="onSeekInput"
            @pointerup="onSeekEnd"
            @pointerleave="onSeekEnd"
            class="w-full h-1.5 rounded-full appearance-none bg-muted cursor-pointer
              [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-4 [&::-webkit-slider-thumb]:h-4
              [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary
              [&::-webkit-slider-thumb]:border-[2.5px] [&::-webkit-slider-thumb]:border-background
              [&::-webkit-slider-thumb]:shadow-md [&::-webkit-slider-thumb]:cursor-pointer
              [&::-webkit-slider-thumb]:transition-transform [&::-webkit-slider-thumb]:duration-150
              [&::-webkit-slider-thumb]:hover:scale-125
              [&::-webkit-slider-track]:bg-muted [&::-webkit-slider-track]:rounded-full
              [&::-webkit-slider-track]:h-1.5"
          />
          <div class="flex justify-between text-xs text-muted-foreground tabular-nums">
            <span>{{ formatTime(currentTime) }}</span>
            <span>{{ formatTime(duration) }}</span>
          </div>
        </div>

        <!-- Controls Row: Speed | Play/Pause (center) | Volume -->
        <div class="flex items-center gap-2">
          <!-- Left: Speed -->
          <div class="flex items-center gap-1 flex-1 justify-start">
            <Button
              v-for="rate in [0.5, 1, 1.5, 2]"
              :key="rate"
              size="sm"
              variant="outline"
              :class="[
                playbackRate === rate
                  ? 'bg-primary text-primary-foreground border-primary shadow-sm scale-105'
                  : 'hover:bg-muted',
                'transition-all duration-200 font-medium text-xs px-2.5'
              ]"
              @click="changeSpeed(rate)"
            >
              <ZapIcon v-if="playbackRate === rate" class="w-3 h-3 mr-0.5" />
              {{ rate }}x
            </Button>
          </div>

          <!-- Center: Play/Pause -->
          <Button size="icon" variant="ghost" @click="togglePlay" class="shrink-0">
            <PlayIcon v-if="!isPlaying" class="w-5 h-5" />
            <PauseIcon v-else class="w-5 h-5" />
          </Button>

          <!-- Right: Volume -->
          <div class="flex items-center gap-1.5 flex-1 justify-end">
            <button
              class="text-muted-foreground hover:text-foreground transition-colors p-1"
              @click="toggleMute"
              title="静音切换"
            >
              <Volume2Icon v-if="volume > 0.5 && !isMuted" class="w-4 h-4" />
              <Volume1Icon v-else-if="volume > 0 && !isMuted" class="w-4 h-4" />
              <VolumeXIcon v-else class="w-4 h-4" />
            </button>
            <input
              type="range"
              min="0"
              max="1"
              step="0.05"
              :value="isMuted ? 0 : volume"
              @input="changeVolume"
              class="w-20 h-1 rounded-full appearance-none bg-muted cursor-pointer
                [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3
                [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary
                [&::-webkit-slider-thumb]:border-2 [&::-webkit-slider-thumb]:border-background
                [&::-webkit-slider-thumb]:shadow-sm [&::-webkit-slider-thumb]:cursor-pointer"
            />
          </div>
        </div>
      </div>

      <!-- Scrollable Original Text Section -->
      <div class="flex-1 overflow-y-auto p-4 space-y-3">
        <div class="flex items-center justify-between">
          <h3 class="text-sm font-semibold text-muted-foreground">原文文本</h3>
          <span class="text-[11px] text-muted-foreground/50 font-mono tabular-nums">
            共 {{ originalText?.length || 0 }} 字
          </span>
        </div>
        <div class="text-sm leading-relaxed whitespace-pre-wrap text-foreground select-text">
          {{ originalText || '暂无文本' }}
        </div>
      </div>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, watch, computed, onUnmounted } from 'vue'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import {
  Play as PlayIcon,
  Pause as PauseIcon,
  Zap as ZapIcon,
  Volume2 as Volume2Icon,
  Volume1 as Volume1Icon,
  VolumeX as VolumeXIcon,
} from 'lucide-vue-next'
import { api } from '@/api/client'
import { toast } from 'vue-sonner'

const props = defineProps<{
  open: boolean
  taskId: string | null
  originalText?: string
}>()

defineEmits<{
  'update:open': [value: boolean]
}>()

// ─── Audio State ──────────────────────────
const audio = ref<HTMLAudioElement | null>(null)
const currentTime = ref(0)
const duration = ref(0)
const isPlaying = ref(false)
const playbackRate = ref(1)
const audioReady = computed(() => duration.value > 0)

// ─── Progress Seek (pointer events, native range reliability) ──
const isSeeking = ref(false)

function onSeekStart() {
  isSeeking.value = true
}

function onSeekInput(e: Event) {
  if (!isSeeking.value || !audio.value || !audioReady.value) return
  const target = e.currentTarget as HTMLInputElement
  const time = parseFloat(target.value)
  audio.value.currentTime = time
  currentTime.value = time
}

function onSeekEnd() {
  if (!isSeeking.value) return
  isSeeking.value = false
  // Final commit — audio picks up from currentTime set by last input event
}

// ─── Volume State ─────────────────────────
const volume = ref(1)
const isMuted = ref(false)
const prevVolume = ref(1)

function changeVolume(e: Event) {
  const val = parseFloat((e.target as HTMLInputElement).value)
  volume.value = val
  isMuted.value = val === 0
  if (audio.value) {
    audio.value.volume = val
  }
}

function toggleMute() {
  if (isMuted.value) {
    isMuted.value = false
    volume.value = prevVolume.value
    if (audio.value) audio.value.volume = prevVolume.value
  } else {
    isMuted.value = true
    prevVolume.value = volume.value
    if (audio.value) audio.value.volume = 0
  }
}

// ─── Audio Loading ────────────────────────
watch(() => props.taskId, async (taskId) => {
  if (taskId && props.open) {
    await loadAudio(taskId)
  }
})

watch(() => props.open, async (isOpen) => {
  if (isOpen && props.taskId) {
    await loadAudio(props.taskId)
  } else {
    cleanup()
  }
})

async function loadAudio(taskId: string) {
  cleanup()

  audio.value = new Audio(api.getAudioUrl(taskId))
  audio.value.volume = isMuted.value ? 0 : volume.value

  audio.value.addEventListener('loadedmetadata', () => {
    if (audio.value) {
      duration.value = audio.value.duration
      audio.value.playbackRate = playbackRate.value
      // play() 会触发 play 事件，由事件监听器设置 isPlaying
      audio.value.play().catch(() => {
        // 自动播放被浏览器阻止，isPlaying 保持 false
      })
    }
  })

  audio.value.addEventListener('timeupdate', () => {
    if (audio.value && !isSeeking.value) {
      currentTime.value = audio.value.currentTime
    }
  })

  audio.value.addEventListener('ended', () => {
    isPlaying.value = false
  })

  // 监听 play/pause 事件来同步状态（解决 autoplay 阻止和异步问题）
  audio.value.addEventListener('play', () => {
    isPlaying.value = true
  })

  audio.value.addEventListener('pause', () => {
    isPlaying.value = false
  })

  // Handle audio loading errors (404, network errors, etc.)
  audio.value.addEventListener('error', (e: Event) => {
    const mediaErr = (e.target as HTMLAudioElement)?.error
    // Ignore errors during cleanup (empty src) or user abort
    if (!mediaErr || mediaErr.code === MediaError.MEDIA_ERR_ABORTED) return
    if (mediaErr.code === MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED && !audio.value?.src) return
    console.error('Audio load error:', mediaErr.message)
    toast.error('音频加载失败', {
      description: '音频文件可能已过期或不存在，请重新合成',
    })
  })
}

function togglePlay() {
  if (!audio.value) return
  if (isPlaying.value) {
    audio.value.pause()
  } else {
    // play() 返回 Promise，浏览器可能阻止 autoplay
    audio.value.play().catch(() => {
      // 自动播放被阻止，状态由 pause 事件回调设置
    })
  }
  // 移除手动翻转 — 由 play/pause 事件监听器处理
}

function changeSpeed(rate: number) {
  playbackRate.value = rate
  if (audio.value) {
    audio.value.playbackRate = rate
  }
}

function formatTime(seconds: number): string {
  const mins = Math.floor(seconds / 60)
  const secs = Math.floor(seconds % 60)
  return `${mins}:${secs.toString().padStart(2, '0')}`
}

function cleanup() {
  if (audio.value) {
    audio.value.pause()
    audio.value.removeAttribute('src')
    audio.value.load() // abort any pending network request
    audio.value = null
  }
  isPlaying.value = false
  currentTime.value = 0
  duration.value = 0
}

onUnmounted(() => {
  cleanup()
})
</script>
