<template>
  <Card class="bg-background/80 dark:bg-background/60 backdrop-blur-xl border-border/50 shadow-lg">
    <CardHeader class="pb-3 px-4 md:px-6 lg:px-8">
      <CardTitle>新建合成任务</CardTitle>
      <CardDescription>输入文本并选择音色进行语音合成</CardDescription>
    </CardHeader>
    <CardContent class="space-y-5 sm:space-y-6 pt-0 px-4 md:px-6 lg:px-8">
      <!-- Task Name Input -->
      <div class="space-y-2">
        <Label for="taskName">任务名称（可选）</Label>
        <Input
          id="taskName"
          v-model="form.taskName"
          placeholder="留空则自动生成（例如：任务_20240519_143022）"
          class="text-sm"
        />
        <p class="text-xs text-muted-foreground">
          相同名称的任务会自动添加序号后缀以避免冲突
        </p>
      </div>

      <!-- Text Input -->
      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <Label for="text" class="text-sm sm:text-base">合成文本 <span class="text-destructive">*</span></Label>
          <Button 
            v-if="form.text" 
            variant="ghost" 
            size="sm"
            class="h-6 text-xs"
            @click="clearText"
          >
            清空
          </Button>
        </div>
        <Textarea
          id="text"
          v-model="form.text"
          placeholder="输入要合成的文本..."
          rows="5"
          class="text-sm sm:text-base"
          @input="updateCounts"
        />
        <div class="text-xs text-muted-foreground flex flex-wrap gap-2 sm:gap-3">
          <span>字符数: {{ charCount }}</span>
          <span>预估 Token: {{ estimatedTokens }}</span>
          <span v-if="estimatedAudioTime" class="text-primary">预估时长: {{ estimatedAudioTime }}</span>
          <span v-if="charCount > 2000" class="text-blue-500">
            将自动分 {{ Math.ceil(charCount / 2000) }} 片均匀合成
          </span>
        </div>
        <div v-if="charCount > 2000" class="text-xs text-yellow-600 dark:text-yellow-400 mt-1">
          ⚠️ 文本较长，将自动分 {{ Math.ceil(charCount / 2000) }} 片（每分钟 10 次 API 限制，约需 {{ Math.ceil(charCount / 2000) * 6 }} 秒）
        </div>
      </div>

      <!-- Model Select -->
      <div class="space-y-2">
        <Label for="model">模型</Label>
        <Select v-model="form.model">
          <SelectTrigger>
            <SelectValue placeholder="选择模型" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="mimo-v2.5-tts">mimo-v2.5-tts (预置音色)</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <!-- Voice Selection -->
      <div class="space-y-2">
        <Label>音色 <span class="text-destructive">*</span></Label>
        
        <div v-if="voicesLoading" class="text-sm text-muted-foreground">加载音色中...</div>
        <div v-else class="space-y-2.5 xl:grid xl:grid-cols-2 xl:gap-2.5 xl:space-y-0">
          <div
            v-for="voice in voices"
            :key="voice.id"
            class="flex items-center justify-between p-4 rounded-lg border-2 cursor-pointer transition-all duration-150 active:scale-[0.98]"
            :class="{
              'border-primary bg-primary-light dark:bg-primary/10': form.voice === voice.id,
              'border-border hover:border-primary/50 hover:bg-muted/50': form.voice !== voice.id
            }"
            @click="form.voice = voice.id"
            tabindex="0"
            @keydown.enter="form.voice = voice.id"
            @keydown.space.prevent="form.voice = voice.id"
          >
            <!-- 左侧：选中标记 + 音色信息 -->
            <div class="flex items-center gap-3 flex-1 min-w-0">
              <!-- 选中标记（仅选中时显示） -->
              <CheckIcon v-if="form.voice === voice.id" 
                         class="w-5 h-5 text-primary shrink-0 mr-1" />
              <div v-else class="w-5 shrink-0 mr-1"></div>
              
              <!-- 音色名称和描述 -->
              <div class="min-w-0 flex-1">
                <div class="font-medium text-sm truncate mb-1.5">{{ voice.name }}</div>
                
                <!-- 图标化属性标签 -->
                <div class="flex items-center gap-2 text-xs text-muted-foreground flex-wrap">
                  <!-- 语言 -->
                  <span class="inline-flex items-center gap-1">
                    <GlobeIcon class="w-3 h-3" />
                    {{ voice.language }}
                  </span>
                  
                  <!-- 性别 -->
                  <span class="inline-flex items-center gap-1">
                    <UserIcon v-if="voice.gender === '男性' || voice.gender === 'Male'" class="w-3 h-3" />
                    <UserRoundIcon v-else class="w-3 h-3" />
                    {{ voice.gender }}
                  </span>
                  
                  <!-- 风格（如果有） -->
                  <span v-if="voice.style" class="inline-flex items-center gap-1">
                    <SparklesIcon class="w-3 h-3" />
                    {{ voice.style }}
                  </span>
                </div>
              </div>
            </div>
            
            <!-- 右侧：播放/暂停按钮（仅在有 CDN 音频 URL 时显示） -->
            <Button
              v-if="voice.preview_url"
              size="sm"
              variant="outline"
              class="h-10 w-10 p-0 shrink-0 ml-3 rounded-full border-2 
                     hover:bg-primary/10 hover:border-primary 
                     active:scale-95 transition-all duration-150
                     disabled:opacity-50 disabled:cursor-not-allowed"
              @click.stop="playVoicePreview(voice.id)"
              :aria-label="previewingVoice === voice.id && !isPaused ? '暂停' : '试听音色'"
            >
              <Loader2Icon v-if="previewingVoice === voice.id && loading" class="w-5 h-5 text-primary animate-spin" />
              <PauseIcon v-else-if="previewingVoice === voice.id && !isPaused" class="w-5 h-5 text-primary" />
              <PlayIcon v-else class="w-5 h-5 text-primary" />
            </Button>
          </div>
        </div>
      </div>

      <!-- Context Input -->
      <div class="space-y-2">
        <Label for="context">风格控制 <span class="text-muted-foreground">(可选)</span></Label>
        <Textarea
          id="context"
          v-model="form.context"
          placeholder="例如：用温柔的语气，语速稍慢&#10;支持多行描述，更精细地控制语音风格"
          rows="3"
          maxlength="1024"
          class="text-sm sm:text-base resize-none"
          @input="updateContextCount"
        />
        <div class="flex items-center justify-between text-xs">
          <span class="text-muted-foreground">
            描述期望的语气、情感和语速风格
          </span>
          <span :class="contextCharCount >= 1024 ? 'text-destructive font-medium' : 'text-muted-foreground'">
            {{ contextCharCount }}/1024
          </span>
        </div>
      </div>

      <!-- Submit Button -->
      <Button
        @click="handleSubmit"
        :disabled="isSubmitting || !form.text.trim() || !form.voice || !configStore.hasValidKey"
        class="w-full sm:w-auto text-sm sm:text-base"
      >
        {{ isSubmitting ? '合成中...' : '开始合成' }}
      </Button>
    </CardContent>
  </Card>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { toast } from 'vue-sonner'
import { useTaskStore } from '@/stores/task'
import { useConfigStore } from '@/stores/config'
import { type Voice } from '@/api/client'
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from '@/components/ui/card'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { 
  Play as PlayIcon, 
  Pause as PauseIcon,
  Loader2 as Loader2Icon, 
  Check as CheckIcon,
  Globe as GlobeIcon,
  User as UserIcon,
  UserRound as UserRoundIcon,
  Sparkles as SparklesIcon
} from 'lucide-vue-next'

const taskStore = useTaskStore()
const configStore = useConfigStore()

const voices = ref<Voice[]>([])
const voicesLoading = ref(false)
const isSubmitting = ref(false)
const previewingVoice = ref<string | null>(null)
const currentAudio = ref<HTMLAudioElement | null>(null)
const isPaused = ref(false)
const loading = ref(false)

const form = ref({
  text: '',
  voice: '',
  model: 'mimo-v2.5-tts',
  context: '',
  taskName: '',  // Optional custom task name
})

const charCount = ref(0)
const estimatedTokens = ref(0)
const contextCharCount = ref(0)

function updateCounts() {
  const text = form.value.text
  charCount.value = text.length
  
  // More accurate token estimation
  const chineseChars = (text.match(/[\u4e00-\u9fff]/g) || []).length
  const englishWords = text.split(/\s+/).filter(w => w.length > 0).length
  const punctuation = (text.match(/[.,!?;:，。！？；：]/g) || []).length
  const numbers = (text.match(/\d+/g) || []).length
  
  // MIMO TTS actual token calculation rules
  estimatedTokens.value = Math.ceil(
    chineseChars * 1.2 +   // Chinese characters
    englishWords * 0.8 +   // English words
    punctuation * 0.3 +    // Punctuation
    numbers * 0.5          // Numbers
  )
}

function updateContextCount() {
  contextCharCount.value = (form.value.context ?? '').length
}

// 预估音频时长（中文约 3-4 字/秒）
const estimatedAudioTime = computed(() => {
  if (charCount.value === 0) return null
  const seconds = Math.ceil(charCount.value / 3.5) // 保守估计
  if (seconds < 60) return `${seconds}秒`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}分${seconds % 60}秒`
  return `${Math.floor(seconds / 3600)}小时${Math.floor((seconds % 3600) / 60)}分`
})

const FALLBACK_VOICES: Voice[] = [
  { id: '冰糖', name: '冰糖', language: '中文', gender: '女性', style: '活泼少女', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/bingtang.wav' },
  { id: '茉莉', name: '茉莉', language: '中文', gender: '女性', style: '知性女声', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/moli.wav' },
  { id: '苏打', name: '苏打', language: '中文', gender: '男性', style: '阳光少年', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/suda.wav' },
  { id: '白桦', name: '白桦', language: '中文', gender: '男性', style: '成熟男声', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/baihua.wav' },
  { id: 'Mia', name: 'Mia', language: 'English', gender: 'Female', style: 'Lively girl', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/mia.wav' },
  { id: 'Chloe', name: 'Chloe', language: 'English', gender: 'Female', style: 'Sweet Dreamy', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/chloe.wav' },
  { id: 'Milo', name: 'Milo', language: 'English', gender: 'Male', style: 'Sunny boy', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/milo.wav' },
  { id: 'Dean', name: 'Dean', language: 'English', gender: 'Male', style: 'Steady Gentle', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/dean.wav' },
]

async function loadVoices() {
  voicesLoading.value = true
  // Backend-next 没有独立音色列表接口，直接使用预置音色
  voices.value = FALLBACK_VOICES
  voicesLoading.value = false
  if (voices.value.length > 0 && !form.value.voice) {
    form.value.voice = voices.value[0].id
  }
}

async function handleSubmit() {
  if (!form.value.text.trim()) {
    toast.error('请输入合成文本')
    return
  }
  if (!form.value.voice) {
    toast.error('请选择音色')
    return
  }
  if (!configStore.hasValidKey) {
    toast.error('API Key 无效或为环境占位符，请重新配置')
    return
  }

  isSubmitting.value = true
  try {
    const taskId = await taskStore.createTask({
      text: form.value.text,
      voice: form.value.voice,
      model: form.value.model,
      context: form.value.context || undefined,
      task_name: form.value.taskName || undefined,  // Pass custom task name
      api_key: configStore.apiKey,
    })
    
    toast.success('任务创建成功')
    
    // Enqueue the task for synthesis processing (v2 two-phase flow)
    await taskStore.enqueueTask(taskId)
    toast.success('任务已加入队列')
    
    form.value.text = ''
    form.value.context = ''
    updateCounts()
    contextCharCount.value = 0
  } catch (error: any) {
    toast.error(error.response?.data?.message || error.message || '创建任务失败')
  } finally {
    isSubmitting.value = false
  }
}

async function playVoicePreview(voiceId: string) {
  const voice = voices.value.find(v => v.id === voiceId)
  
  if (!voice) {
    toast.error('音色不存在')
    return
  }

  // 如果正在播放同一个音色，暂停/恢复播放
  if (previewingVoice.value === voiceId && currentAudio.value) {
    if (currentAudio.value.paused) {
      currentAudio.value.play()
      isPaused.value = false
    } else {
      currentAudio.value.pause()
      isPaused.value = true
    }
    return
  }

  // 停止之前的播放
  if (currentAudio.value) {
    currentAudio.value.pause()
    currentAudio.value = null
  }

  try {
    previewingVoice.value = voiceId
    isPaused.value = false
    loading.value = true
    
    // 优先使用 CDN URL
    const previewUrl = voice.preview_url || ''
    
    const audio = new Audio(previewUrl)
    currentAudio.value = audio
    
    audio.onended = () => {
      previewingVoice.value = null
      currentAudio.value = null
      isPaused.value = false
      loading.value = false
    }
    
    audio.onerror = () => {
      previewingVoice.value = null
      currentAudio.value = null
      isPaused.value = false
      loading.value = false
    }
    
    audio.oncanplaythrough = () => {
      loading.value = false
    }
    
    await audio.play()
  } catch (_error) {
    previewingVoice.value = null
    currentAudio.value = null
    isPaused.value = false
    loading.value = false
  }
}

// Keyboard shortcuts
function handleKeydown(event: KeyboardEvent) {
  // Ctrl/Cmd + Enter to submit
  if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
    event.preventDefault()
    if (!isSubmitting.value && form.value.text.trim() && form.value.voice) {
      handleSubmit()
    }
  }
  
  // ESC to clear text (only when textarea is focused)
  if (event.key === 'Escape') {
    const textArea = document.getElementById('text') as HTMLTextAreaElement
    if (document.activeElement === textArea && form.value.text) {
      event.preventDefault()
      clearText()
    }
  }
}

function clearText() {
  if (confirm('确定要清空文本吗？')) {
    form.value.text = ''
    updateCounts()
  }
}

onMounted(() => {
  loadVoices()
  contextCharCount.value = (form.value.context ?? '').length
  window.addEventListener('keydown', handleKeydown)
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown)
})

// 暴露方法供父组件调用（配置复用）
function setConfig(config: { text: string; voice: string | null; model: string; context?: string; task_name?: string }) {
  if (config.text) form.value.text = config.text
  if (config.voice) form.value.voice = config.voice
  if (config.model) form.value.model = config.model
  if (config.context !== undefined) form.value.context = config.context
  if (config.task_name) form.value.taskName = config.task_name
  updateCounts()
  contextCharCount.value = (form.value.context ?? '').length
  toast.success('已复用历史配置')
}

defineExpose({
  setConfig
})
</script>
