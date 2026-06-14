<template>
  <div v-if="src" class="space-y-3">
    <div class="space-y-1.5">
      <div class="relative h-2 rounded-full bg-muted overflow-hidden">
        <div class="absolute inset-y-0 left-0 rounded-full bg-primary transition-all"
             :style="{ width: `${progressPercent}%` }" />
        <input type="range" min="0" :max="duration || 0" step="0.01"
               :value="currentTime" @input="onSeek"
               class="absolute inset-0 w-full h-full opacity-0 cursor-pointer" />
      </div>
      <div class="flex justify-between text-xs text-muted-foreground">
        <span>{{ formatTime(currentTime) }}</span>
        <span>{{ formatTime(duration) }}</span>
      </div>
    </div>
    <div class="flex items-center justify-center gap-3">
      <Button size="icon" variant="outline" @click="toggle" :disabled="!src">
        <Play v-if="!playing" class="w-4 h-4" />
        <Pause v-else class="w-4 h-4" />
      </Button>
      <div class="flex items-center gap-1">
        <button v-for="r in rates" :key="r" @click="audio.setRate(r)"
                :class="['px-2 py-0.5 rounded text-xs border', audio.playbackRate.value === r ? 'bg-primary text-primary-foreground border-primary' : 'border-border']">
          {{ r }}x
        </button>
      </div>
      <a :href="src" download>
        <Button size="icon" variant="ghost"><Download class="w-4 h-4" /></Button>
      </a>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Play, Pause, Download } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { useAudio } from '@/composables/useAudio'
import { formatTime } from '@/utils/format'

const props = defineProps<{ src: string }>()
const audio = useAudio()
const rates = [0.5, 0.75, 1, 1.25, 1.5, 2]

const playing = audio.playing
const currentTime = audio.currentTime
const duration = audio.duration
const progressPercent = computed(() => {
  const d = duration.value
  const c = currentTime.value
  return d > 0 ? (c / d) * 100 : 0
})

function toggle() { audio.play(props.src) }
function onSeek(e: Event) { audio.seek(Number((e.target as HTMLInputElement).value)) }
</script>
