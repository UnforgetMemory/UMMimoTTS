<template>
  <Dialog :open="open" @update:open="$emit('update:open', $event)">
    <DialogContent class="sm:max-w-3xl max-h-[90vh] overflow-hidden flex flex-col">
      <DialogHeader class="shrink-0">
        <DialogTitle>音频播放器</DialogTitle>
      </DialogHeader>
      
      <!-- Fixed Audio Player Section -->
      <div class="shrink-0 space-y-4 p-4 border-b bg-card">
        <!-- Waveform Visualization -->
        <canvas 
          ref="waveformCanvas" 
          class="w-full h-32 bg-muted rounded-lg"
        ></canvas>
        
        <!-- Progress Slider -->
        <Slider
          v-model="currentTimeValue"
          :max="duration"
          :step="0.01"
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
            v-for="rate in [0.5, 1, 1.5, 2]"
            :key="rate"
            size="sm" 
            variant="outline"
            :class="[
              playbackRate === rate ? 'bg-primary text-primary-foreground border-primary shadow-md scale-105' : 'hover:bg-muted',
              'transition-all duration-200 font-medium'
            ]"
            @click="changeSpeed(rate)"
          >
            <ZapIcon v-if="playbackRate === rate" class="w-3 h-3 mr-1" />
            {{ rate }}x
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
      
      <!-- Scrollable Original Text Section -->
      <div class="flex-1 overflow-y-auto p-4 space-y-3">
        <h3 class="text-sm font-semibold text-muted-foreground">原文文本</h3>
        <div class="text-sm leading-relaxed whitespace-pre-wrap text-foreground">
          {{ props.originalText || '暂无文本' }}
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
import { Play as PlayIcon, Pause as PauseIcon, Zap as ZapIcon } from 'lucide-vue-next'
import { api } from '@/api/client'

const props = defineProps<{
  open: boolean
  taskId: string | null
  originalText?: string  // Original text for display
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
      
      // Auto-play when audio is loaded
      audio.value.play().then(() => {
        isPlaying.value = true
      }).catch(err => {
        console.warn('Auto-play prevented by browser policy:', err)
        // Browser may block auto-play, fail silently
      })
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
  
  // High DPI support for Retina displays
  const dpr = window.devicePixelRatio || 1
  const rect = canvas.getBoundingClientRect()
  canvas.width = rect.width * dpr
  canvas.height = rect.height * dpr
  ctx.scale(dpr, dpr)
  
  try {
    const response = await fetch(url, { mode: 'cors' })
    if (!response.ok) throw new Error(`HTTP ${response.status}`)
    
    const arrayBuffer = await response.arrayBuffer()
    const audioContext = new AudioContext()
    audioContextRef.value = audioContext
    const audioBuffer = await audioContext.decodeAudioData(arrayBuffer)
    
    const channelData = audioBuffer.getChannelData(0)
    const samples = 200 // More samples for smoother visualization
    const blockSize = Math.floor(channelData.length / samples)
    
    // Gradient background
    const gradient = ctx.createLinearGradient(0, 0, 0, rect.height)
    gradient.addColorStop(0, 'hsl(var(--primary) / 0.1)')
    gradient.addColorStop(0.5, 'hsl(var(--primary) / 0.3)')
    gradient.addColorStop(1, 'hsl(var(--primary) / 0.1)')
    
    ctx.fillStyle = gradient
    ctx.fillRect(0, 0, rect.width, rect.height)
    
    // Draw mirrored waveform
    const barWidth = rect.width / samples
    const centerY = rect.height / 2
    
    for (let i = 0; i < samples; i++) {
      let sum = 0
      for (let j = 0; j < blockSize; j++) {
        sum += Math.abs(channelData[i * blockSize + j])
      }
      const average = sum / blockSize
      const barHeight = average * centerY * 1.8
      
      const x = i * barWidth
      const yTop = centerY - barHeight / 2
      const yBottom = centerY + barHeight / 2
      
      // Gradient for bars
      const barGradient = ctx.createLinearGradient(x, yTop, x, yBottom)
      barGradient.addColorStop(0, 'hsl(var(--primary) / 0.6)')
      barGradient.addColorStop(0.5, 'hsl(var(--primary))')
      barGradient.addColorStop(1, 'hsl(var(--primary) / 0.6)')
      
      ctx.fillStyle = barGradient
      ctx.fillRect(x, yTop, barWidth - 1, barHeight)
    }
    
    // Add center line
    ctx.strokeStyle = 'hsl(var(--primary) / 0.3)'
    ctx.lineWidth = 1
    ctx.beginPath()
    ctx.moveTo(0, centerY)
    ctx.lineTo(rect.width, centerY)
    ctx.stroke()
    
    await audioContext.close()
    
  } catch (error) {
    console.warn('波形可视化失败，使用降级方案:', error)
    drawEnhancedFallbackWaveform(ctx, rect)
  }
}

// Enhanced fallback with animation
function drawEnhancedFallbackWaveform(ctx: CanvasRenderingContext2D, rect: DOMRect) {
  ctx.clearRect(0, 0, rect.width, rect.height)
  
  // Background gradient
  const bgGradient = ctx.createLinearGradient(0, 0, 0, rect.height)
  bgGradient.addColorStop(0, 'hsl(var(--muted))')
  bgGradient.addColorStop(1, 'hsl(var(--background))')
  ctx.fillStyle = bgGradient
  ctx.fillRect(0, 0, rect.width, rect.height)
  
  // Animated sine wave
  const centerY = rect.height / 2
  const amplitude = rect.height / 4
  const time = Date.now() / 1000
  
  ctx.strokeStyle = 'hsl(var(--primary))'
  ctx.lineWidth = 2
  ctx.lineCap = 'round'
  
  ctx.beginPath()
  for (let x = 0; x < rect.width; x++) {
    const y = centerY + 
      Math.sin(x * 0.02 + time * 2) * amplitude * 0.5 +
      Math.sin(x * 0.05 + time * 3) * amplitude * 0.3
    if (x === 0) {
      ctx.moveTo(x, y)
    } else {
      ctx.lineTo(x, y)
    }
  }
  ctx.stroke()
  
  // Hint text
  ctx.fillStyle = 'hsl(var(--muted-foreground))'
  ctx.font = '12px sans-serif'
  ctx.textAlign = 'center'
  ctx.fillText('波形预览（简化模式）', rect.width / 2, rect.height - 10)
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
