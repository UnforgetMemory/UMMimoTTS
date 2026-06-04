<template>
  <Dialog :open="open" @update:open="onOpenChange">
    <DialogContent class="sm:max-w-3xl max-h-[90vh] overflow-hidden flex flex-col">
      <DialogHeader class="shrink-0">
        <DialogTitle>音频播放器</DialogTitle>
        <DialogDescription class="sr-only">TTS 音频播放器</DialogDescription>
      </DialogHeader>

      <!-- Fixed Audio Player Section -->
      <div class="shrink-0 space-y-4 p-4 pb-3 border-b bg-card">
        <!-- Progress Bar -->
        <div class="space-y-1.5" :class="{ 'opacity-40 pointer-events-none': !(audioStore.duration > 0) }">
          <input
            type="range"
            min="0"
            :max="audioStore.duration || 0"
            step="0.01"
            :value="isSeeking ? seekValue : audioStore.currentTime"
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
            <span>{{ formatTime(audioStore.currentTime) }}</span>
            <span>{{ formatTime(audioStore.duration) }}</span>
          </div>
        </div>

        <!-- Controls Row: Speed | Play/Pause | Volume -->
        <div class="flex items-center gap-2">
          <!-- Left: Speed -->
          <div class="flex items-center gap-1 flex-1 justify-start flex-wrap">
            <Button
              v-for="rate in SPEEDS"
              :key="rate"
              size="sm"
              variant="outline"
              :class="[
                audioStore.playbackRate === rate
                  ? 'bg-primary text-primary-foreground border-primary shadow-sm scale-105'
                  : 'hover:bg-muted',
                'transition-all duration-200 font-medium text-[11px] px-1.5 h-7 min-w-[38px]'
              ]"
              @click="audioStore.changeSpeed(rate)"
            >
              {{ rate }}x
            </Button>
          </div>

          <!-- Center: Play/Pause -->
          <Button size="icon" variant="ghost" @click="audioStore.toggle()" class="shrink-0">
            <PlayIcon v-if="!audioStore.isPlaying" class="w-5 h-5" />
            <PauseIcon v-else class="w-5 h-5" />
          </Button>

          <!-- Right: Volume -->
          <div class="flex items-center gap-1.5 flex-1 justify-end">
            <button
              class="text-muted-foreground hover:text-foreground transition-colors p-1"
              @click="audioStore.toggleMute()"
              title="静音切换"
            >
              <Volume2Icon v-if="audioStore.volume > 0.5 && !audioStore.isMuted" class="w-4 h-4" />
              <Volume1Icon v-else-if="audioStore.volume > 0 && !audioStore.isMuted" class="w-4 h-4" />
              <VolumeXIcon v-else class="w-4 h-4" />
            </button>
            <input
              type="range"
              min="0"
              max="1"
              step="0.05"
              :value="audioStore.isMuted ? 0 : audioStore.volume"
              @input="onVolumeChange"
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
import { ref, watch, onUnmounted } from 'vue'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import {
  Play as PlayIcon,
  Pause as PauseIcon,
  Volume2 as Volume2Icon,
  Volume1 as Volume1Icon,
  VolumeX as VolumeXIcon,
} from 'lucide-vue-next'
import { api } from '@/api/client'
import { useAudioStore } from '@/stores/audio'

const props = defineProps<{
  open: boolean
  taskId: string | null
  originalText?: string
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
}>()

const audioStore = useAudioStore()

const SPEEDS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2, 3, 6]

// ─── Seek (pointer events for smooth dragging) ──
const isSeeking = ref(false)
const seekValue = ref(0)

function onSeekStart() {
  isSeeking.value = true
}

function onSeekInput(e: Event) {
  const time = parseFloat((e.currentTarget as HTMLInputElement).value)
  seekValue.value = time
  audioStore.seek(time)
}

function onSeekEnd() {
  isSeeking.value = false
}

// ─── Volume ───────────────────────────────────
function onVolumeChange(e: Event) {
  const val = parseFloat((e.currentTarget as HTMLInputElement).value)
  audioStore.changeVolume(val)
}

// ─── Audio loading ────────────────────────────
async function loadAudio(taskId: string) {
  const url = api.getAudioUrl(taskId)
  await audioStore.play(url)
}

function onOpenChange(open: boolean) {
  if (!open) {
    audioStore.pause()
  }
  emit('update:open', open)
}

watch(() => props.taskId, async (taskId) => {
  if (taskId && props.open) {
    await loadAudio(taskId)
  }
})

watch(() => props.open, async (isOpen) => {
  if (isOpen && props.taskId) {
    await loadAudio(props.taskId)
  } else if (!isOpen) {
    audioStore.stop()
  }
})

// ─── Helpers ──────────────────────────────────
function formatTime(seconds: number): string {
  const mins = Math.floor(seconds / 60)
  const secs = Math.floor(seconds % 60)
  return `${mins}:${secs.toString().padStart(2, '0')}`
}

onUnmounted(() => {
  audioStore.stop()
})
</script>
