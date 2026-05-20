<template>
  <Dialog :open="open" @update:open="$emit('update:open', $event)">
    <DialogContent class="sm:max-w-2xl">
      <DialogHeader>
        <DialogTitle>音频播放器</DialogTitle>
      </DialogHeader>
      
      <div class="space-y-4">
        <!-- Waveform Visualization -->
        <canvas 
          ref="waveformCanvas" 
          class="w-full h-32 bg-muted rounded-lg"
        ></canvas>
        
        <!-- Progress Slider -->
        <Slider
          v-model="currentTimeValue"
          :max="duration"
          :step="0.1"
          @update:model-value="seekTo"
        />
        
        <!-- Time Display -->
        <div class="flex justify-between text-xs text-muted-foreground">
          <span>{{ formatTime(currentTime) }}</span>
          <span>{{ formatTime(duration) }}</span>
        </div>
        
        <!-- Playback Speed Controls -->
        <div class="flex items-center justify-center gap-2">
          <Button 
            size="sm" 
            variant="outline"
            :class="{ 'bg-primary text-primary-foreground': playbackRate === 0.5 }"
            @click="changeSpeed(0.5)"
          >
            0.5x
          </Button>
          <Button 
            size="sm" 
            variant="outline"
            :class="{ 'bg-primary text-primary-foreground': playbackRate === 1 }"
            @click="changeSpeed(1)"
          >
            1x
          </Button>
          <Button 
            size="sm" 
            variant="outline"
            :class="{ 'bg-primary text-primary-foreground': playbackRate === 1.5 }"
            @click="changeSpeed(1.5)"
          >
            1.5x
          </Button>
          <Button 
            size="sm" 
            variant="outline"
            :class="{ 'bg-primary text-primary-foreground': playbackRate === 2 }"
            @click="changeSpeed(2)"
          >
            2x
          </Button>
        </div>
        
        <!-- Play/Pause Button -->
        <div class="flex items-center justify-center gap-2">
          <Button size="lg" @click="togglePlay">
            <PlayIcon v-if="!isPlaying" class="w-5 h-5" />
            <PauseIcon v-else class="w-5 h-5" />
          </Button>
        </div>
      </div>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, watch, onUnmounted } from 'vue'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Slider } from '@/components/ui/slider'
import { Play as PlayIcon, Pause as PauseIcon } from 'lucide-vue-next'
import { api } from '@/api/client'

const props = defineProps<{
  open: boolean
  taskId: string | null
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
}>()

const audio = ref<HTMLAudioElement | null>(null)
const currentTime = ref(0)
const duration = ref(0)
const isPlaying = ref(false)
const playbackRate = ref(1)
const waveformCanvas = ref<HTMLCanvasElement | null>(null)
const currentTimeValue = ref([0])
const audioContextRef = ref<AudioContext | null>(null)

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
  audio.value.addEventListener('loadedmetadata', () => {
    if (audio.value) {
      duration.value = audio.value.duration
      drawWaveform(api.getAudioUrl(taskId))
    }
  })
  audio.value.addEventListener('timeupdate', () => {
    if (audio.value) {
      currentTime.value = audio.value.currentTime
      currentTimeValue.value = [audio.value.currentTime]
    }
  })
  audio.value.addEventListener('ended', () => {
    isPlaying.value = false
    currentTime.value = 0
    currentTimeValue.value = [0]
  })
}

function togglePlay() {
  if (!audio.value) return
  
  if (isPlaying.value) {
    audio.value.pause()
  } else {
    audio.value.play().catch(err => {
      console.error('Playback failed:', err)
    })
  }
  isPlaying.value = !isPlaying.value
}

function seekTo(value?: number[]) {
  if (!value || !audio.value) return
  audio.value.currentTime = value[0]
  currentTime.value = value[0]
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

async function drawWaveform(url: string) {
  if (!waveformCanvas.value) return
  
  const canvas = waveformCanvas.value
  const ctx = canvas.getContext('2d')
  if (!ctx) return
  
  // Set canvas size
  canvas.width = canvas.offsetWidth
  canvas.height = canvas.offsetHeight
  
  try {
    // Fetch audio data with CORS
    const response = await fetch(url, { mode: 'cors' })
    
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`)
    }
    
    const arrayBuffer = await response.arrayBuffer()
    
    // Decode audio
    const audioContext = new AudioContext()
    audioContextRef.value = audioContext
    const audioBuffer = await audioContext.decodeAudioData(arrayBuffer)
    
    // Get channel data
    const channelData = audioBuffer.getChannelData(0)
    const samples = 100 // Number of bars
    const blockSize = Math.floor(channelData.length / samples)
    
    // Clear canvas
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    
    // Draw waveform
    const barWidth = canvas.width / samples
    const amplitude = canvas.height / 2
    
    ctx.fillStyle = 'hsl(var(--primary))'
    
    for (let i = 0; i < samples; i++) {
      let sum = 0
      for (let j = 0; j < blockSize; j++) {
        sum += Math.abs(channelData[i * blockSize + j])
      }
      const average = sum / blockSize
      const barHeight = average * amplitude * 2
      
      const x = i * barWidth
      const y = (canvas.height - barHeight) / 2
      
      ctx.fillRect(x, y, barWidth - 1, barHeight)
    }
    
    // Important: close AudioContext to free resources
    await audioContext.close()
    
  } catch (error) {
    console.warn('波形可视化失败，使用降级方案:', error)
    drawFallbackWaveform(ctx, canvas)
  }
}

// Fallback: draw simple sine wave pattern
function drawFallbackWaveform(ctx: CanvasRenderingContext2D, canvas: HTMLCanvasElement) {
  ctx.clearRect(0, 0, canvas.width, canvas.height)
  ctx.strokeStyle = 'hsl(var(--primary))'
  ctx.lineWidth = 2
  
  const centerY = canvas.height / 2
  const amplitude = canvas.height / 4
  const frequency = 0.02
  
  ctx.beginPath()
  for (let x = 0; x < canvas.width; x++) {
    const y = centerY + Math.sin(x * frequency) * amplitude * Math.sin(x * 0.01)
    if (x === 0) {
      ctx.moveTo(x, y)
    } else {
      ctx.lineTo(x, y)
    }
  }
  ctx.stroke()
  
  // Add hint text
  ctx.fillStyle = 'hsl(var(--muted-foreground))'
  ctx.font = '12px sans-serif'
  ctx.textAlign = 'center'
  ctx.fillText('波形预览（简化模式）', canvas.width / 2, canvas.height - 10)
}

function cleanup() {
  if (audio.value) {
    audio.value.pause()
    audio.value.src = ''
    audio.value = null
  }
  if (audioContextRef.value) {
    audioContextRef.value.close()
    audioContextRef.value = null
  }
  isPlaying.value = false
  currentTime.value = 0
  duration.value = 0
  currentTimeValue.value = [0]
}

onUnmounted(() => {
  cleanup()
})
</script>
