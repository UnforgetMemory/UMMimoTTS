<template>
  <Card class="bg-background/80 dark:bg-background/60 backdrop-blur-xl border-border/50 shadow-lg flex flex-col h-full">
    <!-- 首行：标题 + 标签页切换 -->
    <CardHeader class="pb-2 px-4 md:px-6 lg:px-8">
      <div class="flex items-center justify-between">
        <CardTitle class="text-lg">合成任务</CardTitle>
        
        <!-- 标签页切换按钮组 -->
        <div class="flex items-center gap-1 bg-muted p-1 rounded-lg">
          <button
            class="py-1 px-3 rounded-md text-sm font-medium transition-colors"
            :class="activeTab === 'control' ? 'bg-background shadow-sm' : 'hover:bg-background/50'"
            @click="activeTab = 'control'"
          >
            控制
          </button>
          <button
            class="py-1 px-3 rounded-md text-sm font-medium transition-colors"
            :class="activeTab === 'config' ? 'bg-background shadow-sm' : 'hover:bg-background/50'"
            @click="activeTab = 'config'"
          >
            配置
          </button>
        </div>
      </div>
    </CardHeader>

    <!-- 主内容区 -->
    <CardContent class="flex-1 flex flex-col pt-0 px-4 md:px-6 lg:px-8 pb-4 overflow-hidden">
      <!-- 控制标签页 -->
      <div v-if="activeTab === 'control'" class="flex-1 flex flex-col">
        <!-- 模型 & 音色徽章 -->
        <div class="flex items-center gap-2 mb-3 flex-wrap">
          <Badge variant="secondary" class="text-xs gap-1 border border-violet-200 dark:border-violet-800 bg-violet-50 dark:bg-violet-950/30 text-violet-700 dark:text-violet-300">
            <SparklesIcon class="w-3 h-3" />
            {{ form.model }}
          </Badge>
          <Badge 
            v-if="selectedVoice" 
            variant="outline" 
            class="text-xs gap-1"
            :class="selectedVoice.gender === '男性' || selectedVoice.gender === 'Male'
              ? 'border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-950/30 text-blue-700 dark:text-blue-300'
              : 'border-pink-200 dark:border-pink-800 bg-pink-50 dark:bg-pink-950/30 text-pink-700 dark:text-pink-300'"
          >
            <UserIcon v-if="selectedVoice.gender === '男性' || selectedVoice.gender === 'Male'" class="w-3 h-3 text-blue-500" />
            <UserRoundIcon v-else class="w-3 h-3 text-pink-500" />
            {{ selectedVoice.name }}
          </Badge>
          <Badge v-else variant="destructive" class="text-xs">
            请选择音色
          </Badge>
        </div>

        <!-- 任务名称 -->
        <div class="mb-3">
          <Input
            v-model="form.taskName"
            placeholder="任务名称（可选）"
            class="text-sm h-9"
          />
        </div>

        <!-- 文本合成区域 -->
        <div class="flex-1 flex flex-col min-h-[200px] mb-3">
          <div class="flex items-center justify-between mb-1.5">
            <Label for="text" class="text-sm">
              合成文本 <span class="text-destructive">*</span>
            </Label>
            <div class="flex items-center gap-2">
              <span class="text-xs text-muted-foreground">{{ charCount }} 字</span>
              <Button 
                v-if="form.text" 
                variant="ghost" 
                size="sm"
                class="h-6 text-xs px-2"
                @click="clearText"
              >
                清空
              </Button>
            </div>
          </div>
          <Textarea
            id="text"
            v-model="form.text"
            placeholder="输入要合成的文本..."
            class="text-sm w-full"
            :style="{ minHeight: '150px', height: 'calc(100% - 50px)' }"
            @input="updateCounts"
          />
          <div class="flex items-center justify-between mt-1.5">
            <div class="text-xs text-muted-foreground flex gap-3">
              <span v-if="estimatedTokens">Token: {{ estimatedTokens }}</span>
              <span v-if="estimatedAudioTime" class="text-primary">{{ estimatedAudioTime }}</span>
            </div>
            <div v-if="charCount > 2000" class="text-xs text-yellow-600 dark:text-yellow-400">
              自动分 {{ Math.ceil(charCount / 2000) }} 片
            </div>
          </div>
        </div>

        <!-- 风格控制 -->
        <div class="mb-4">
          <div class="flex items-center justify-between mb-1.5">
            <Label for="context" class="text-sm">风格控制 <span class="text-muted-foreground text-xs">(可选)</span></Label>
            <span class="text-xs text-muted-foreground">{{ contextCharCount }}/1024</span>
          </div>
          <Textarea
            id="context"
            v-model="form.context"
            placeholder="例如：用温柔的语气，语速稍慢"
            rows="2"
            maxlength="1024"
            class="text-sm resize-none"
            @input="updateContextCount"
          />
        </div>

        <!-- 提交按钮 -->
        <Button
          @click="handleSubmit"
          :disabled="isSubmitting || !form.text.trim() || !form.voice || !configStore.hasValidKey"
          class="w-full h-10"
        >
          <Loader2Icon v-if="isSubmitting" class="w-4 h-4 mr-2 animate-spin" />
          {{ isSubmitting ? '提交中...' : '开始合成' }}
        </Button>
      </div>

      <!-- 配置标签页 -->
      <div v-if="activeTab === 'config'" class="flex-1 flex flex-col overflow-hidden">
        <!-- 配置子标签 -->
        <div class="flex gap-1 mb-3 bg-muted p-1 rounded-lg">
          <button
            class="flex-1 py-1.5 px-3 rounded-md text-sm font-medium transition-colors"
            :class="configTab === 'model' ? 'bg-background shadow-sm' : 'hover:bg-background/50'"
            @click="configTab = 'model'"
          >
            模型
          </button>
          <button
            class="flex-1 py-1.5 px-3 rounded-md text-sm font-medium transition-colors"
            :class="configTab === 'voice' ? 'bg-background shadow-sm' : 'hover:bg-background/50'"
            @click="configTab = 'voice'"
          >
            音色
          </button>
        </div>

        <!-- 模型选择 -->
        <div v-if="configTab === 'model'" class="flex-1 overflow-y-auto grid grid-cols-1 sm:grid-cols-2 gap-2 pr-1 content-start">
          <div
            v-for="model in models"
            :key="model.id"
            class="relative flex flex-col items-center text-center p-3 rounded-lg border cursor-pointer transition-all"
            :class="form.model === model.id 
              ? 'bg-primary/5 border-primary/50 shadow-sm' 
              : 'hover:bg-muted/50'"
            @click="form.model = model.id"
          >
            <div class="w-10 h-10 rounded-lg bg-primary/10 flex items-center justify-center mb-2">
              <SparklesIcon class="w-5 h-5 text-primary" />
            </div>
            <div class="text-sm font-medium">{{ model.name }}</div>
            <p class="text-xs text-muted-foreground mt-1 line-clamp-2">{{ model.description }}</p>
            <div v-if="form.model === model.id" class="absolute top-2 right-2">
              <CheckIcon class="w-4 h-4 text-primary" />
            </div>
          </div>
        </div>

        <!-- 音色选择 -->
        <div v-if="configTab === 'voice'" class="flex-1 overflow-y-auto grid grid-cols-1 sm:grid-cols-2 gap-3 pr-1 content-start">
          <div v-if="voicesLoading" class="col-span-1 sm:col-span-2 flex items-center justify-center py-8">
            <Loader2Icon class="w-5 h-5 animate-spin text-muted-foreground" />
          </div>
          <div
            v-for="voice in voices"
            :key="voice.id"
            class="relative group flex flex-col rounded-xl border cursor-pointer transition-all overflow-hidden"
            :class="form.voice === voice.id 
              ? 'bg-primary/5 border-primary/50 shadow-sm ring-1 ring-primary/20' 
              : 'hover:bg-muted/50 hover:border-muted-foreground/20'"
            @click="form.voice = voice.id"
          >
            <!-- 头部：图标 + 名称 -->
            <div class="flex items-center gap-3 p-3 pb-2">
              <div 
                class="w-9 h-9 rounded-lg flex items-center justify-center shrink-0 transition-colors"
                :class="[
                  voice.gender === '男性' || voice.gender === 'Male' 
                    ? 'bg-blue-100 dark:bg-blue-900/30' 
                    : 'bg-pink-100 dark:bg-pink-900/30',
                  form.voice === voice.id && 'ring-2 ring-primary/30'
                ]"
              >
                <UserIcon v-if="voice.gender === '男性' || voice.gender === 'Male'" class="w-4 h-4 text-blue-500 dark:text-blue-400" />
                <UserRoundIcon v-else class="w-4 h-4 text-pink-500 dark:text-pink-400" />
              </div>
              <div class="flex-1 min-w-0">
                <div class="text-sm font-medium truncate">{{ voice.name }}</div>
                <Badge variant="secondary" class="text-[10px] px-1 mt-0.5">{{ voice.language }}</Badge>
              </div>
              <div v-if="form.voice === voice.id" class="shrink-0">
                <div class="w-5 h-5 rounded-full bg-primary flex items-center justify-center">
                  <CheckIcon class="w-3 h-3 text-primary-foreground" />
                </div>
              </div>
            </div>

            <!-- 中部：风格描述 -->
            <div class="px-3 pb-2">
              <p class="text-xs text-muted-foreground line-clamp-2 leading-relaxed">{{ voice.style }}</p>
            </div>

            <!-- 底部：试听按钮 -->
            <div class="px-3 pb-3 mt-auto">
              <Button
                v-if="voice.preview_url"
                size="sm"
                :variant="previewingVoice === voice.id ? 'default' : 'outline'"
                class="w-full h-7 text-xs opacity-0 group-hover:opacity-100 transition-all duration-200"
                :class="[
                  form.voice === voice.id && 'opacity-100',
                  previewingVoice === voice.id && 'opacity-100 bg-primary/90'
                ]"
                @click.stop="playVoicePreview(voice.id)"
              >
                <Loader2Icon v-if="previewingVoice === voice.id" class="w-3 h-3 mr-1 animate-spin" />
                <PlayIcon v-else class="w-3 h-3 mr-1" />
                {{ previewingVoice === voice.id ? '播放中...' : '试听' }}
              </Button>
            </div>
          </div>
        </div>
      </div>
    </CardContent>
  </Card>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { toast } from 'vue-sonner'
import { useTaskStore } from '@/stores/task'
import { useConfigStore } from '@/stores/config'
import { type Voice } from '@/api/client'
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { 
  Play as PlayIcon, 
  Loader2 as Loader2Icon, 
  Check as CheckIcon,
  User as UserIcon,
  UserRound as UserRoundIcon,
  Sparkles as SparklesIcon
} from 'lucide-vue-next'

const taskStore = useTaskStore()
const configStore = useConfigStore()

const emit = defineEmits<{
  submitted: [taskId: string]
}>()

const models = [
  { id: 'mimo-v2.5-tts', name: 'mimo-v2.5-tts', description: '小米 MIMO TTS 模型，支持预置音色' },
]

const voices = ref<Voice[]>([])
const voicesLoading = ref(false)
const isSubmitting = ref(false)
const previewingVoice = ref<string | null>(null)
const currentAudio = ref<HTMLAudioElement | null>(null)
const loading = ref(false)

const activeTab = ref<'control' | 'config'>('control')
const configTab = ref<'model' | 'voice'>('model')

const form = ref({
  text: '',
  voice: '',
  model: 'mimo-v2.5-tts',
  context: '',
  taskName: '',
})

const charCount = ref(0)
const estimatedTokens = ref(0)
const contextCharCount = ref(0)

const selectedVoice = computed(() => {
  return voices.value.find(v => v.id === form.value.voice)
})

function updateCounts() {
  const text = form.value.text
  charCount.value = text.length
  
  const chineseChars = (text.match(/[\u4e00-\u9fff]/g) || []).length
  const englishWords = text.split(/\s+/).filter(w => w.length > 0).length
  const punctuation = (text.match(/[.,!?;:，。！？；：]/g) || []).length
  const numbers = (text.match(/\d+/g) || []).length
  
  estimatedTokens.value = Math.ceil(
    chineseChars * 1.2 + englishWords * 0.8 + punctuation * 0.3 + numbers * 0.5
  )
}

function updateContextCount() {
  contextCharCount.value = (form.value.context ?? '').length
}

const estimatedAudioTime = computed(() => {
  if (charCount.value === 0) return null
  const seconds = Math.ceil(charCount.value / 3.5)
  if (seconds < 60) return `~${seconds}秒`
  if (seconds < 3600) return `~${Math.floor(seconds / 60)}分${seconds % 60}秒`
  return `~${Math.floor(seconds / 3600)}小时${Math.floor((seconds % 3600) / 60)}分`
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
  voices.value = FALLBACK_VOICES
  voicesLoading.value = false
  if (voices.value.length > 0 && !form.value.voice) {
    form.value.voice = voices.value[0].id
  }
}

function clearText() {
  form.value.text = ''
  charCount.value = 0
  estimatedTokens.value = 0
}

async function playVoicePreview(voiceId: string) {
  const voice = voices.value.find(v => v.id === voiceId)
  if (!voice) {
    toast.error('音色不存在')
    return
  }

  if (previewingVoice.value === voiceId && currentAudio.value) {
    currentAudio.value.pause()
    currentAudio.value.currentTime = 0
    currentAudio.value = null
    previewingVoice.value = null
    return
  }

  if (currentAudio.value) {
    currentAudio.value.pause()
    currentAudio.value = null
  }

  try {
    previewingVoice.value = voiceId
    loading.value = true
    
    const previewUrl = voice.preview_url || ''
    const audio = new Audio(previewUrl)
    currentAudio.value = audio
    
    audio.onended = () => {
      previewingVoice.value = null
      currentAudio.value = null
      loading.value = false
    }
    
    audio.onerror = () => {
      previewingVoice.value = null
      currentAudio.value = null
      loading.value = false
    }
    
    audio.oncanplaythrough = () => {
      loading.value = false
    }
    
    await audio.play()
  } catch (_error) {
    previewingVoice.value = null
    currentAudio.value = null
    loading.value = false
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
    toast.error('请先配置 API Key')
    return
  }

  isSubmitting.value = true
  try {
    const result = await taskStore.createTask({
      text: form.value.text.trim(),
      voice: form.value.voice,
      model: form.value.model,
      task_name: form.value.taskName || undefined,
      context: form.value.context || undefined,
    })
    toast.success('任务已创建')
    form.value.text = ''
    form.value.taskName = ''
    form.value.context = ''
    charCount.value = 0
    estimatedTokens.value = 0
    contextCharCount.value = 0
    emit('submitted', result)
  } catch (error: any) {
    toast.error(error.message || '创建任务失败')
  } finally {
    isSubmitting.value = false
  }
}

function setConfig(config: { text: string; voice: string | null; model: string; context?: string }) {
  form.value.text = config.text
  if (config.voice) form.value.voice = config.voice
  if (config.model) form.value.model = config.model
  if (config.context !== undefined) form.value.context = config.context
  updateCounts()
  updateContextCount()
}

defineExpose({ setConfig })

function handleKeydown(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
    event.preventDefault()
    if (!isSubmitting.value && form.value.text.trim() && form.value.voice) {
      handleSubmit()
    }
  }
}

onMounted(() => {
  loadVoices()
  document.addEventListener('keydown', handleKeydown)
})

onUnmounted(() => {
  document.removeEventListener('keydown', handleKeydown)
  if (currentAudio.value) {
    currentAudio.value.pause()
    currentAudio.value = null
  }
})
</script>
