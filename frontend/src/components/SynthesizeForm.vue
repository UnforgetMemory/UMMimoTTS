<template>
  <Card class="bg-background/80 dark:bg-background/60 backdrop-blur-xl border-border/50 shadow-lg">
    <CardHeader class="pb-3">
      <CardTitle>新建合成任务</CardTitle>
      <CardDescription>输入文本并选择音色进行语音合成</CardDescription>
    </CardHeader>
    <CardContent class="space-y-5 sm:space-y-6 pt-0 px-4">
      <!-- Text Input -->
      <div class="space-y-2">
        <Label for="text" class="text-sm sm:text-base">合成文本 <span class="text-destructive">*</span></Label>
        <Textarea
          id="text"
          v-model="form.text"
          placeholder="输入要合成的文本..."
          rows="4"
          class="text-sm sm:text-base"
          @input="updateCounts"
        />
        <div class="text-xs text-muted-foreground flex flex-wrap gap-2 sm:gap-3">
          <span>字符数: {{ charCount }}</span>
          <span>预估 Token: {{ estimatedTokens }}</span>
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
                    <UserIcon v-if="voice.gender === '男声'" class="w-3 h-3" />
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
            
            <!-- 右侧：播放按钮（固定显示） -->
            <Button
              size="sm"
              variant="outline"
              class="h-10 w-10 p-0 shrink-0 ml-3 rounded-full border-2 
                     hover:bg-primary/10 hover:border-primary 
                     active:scale-95 transition-all duration-150
                     disabled:opacity-50 disabled:cursor-not-allowed"
              @click.stop="playVoicePreview(voice.id)"
              :disabled="previewingVoice === voice.id"
              aria-label="试听音色"
            >
              <PlayIcon v-if="previewingVoice !== voice.id" class="w-5 h-5 text-primary" />
              <Loader2Icon v-else class="w-5 h-5 text-primary animate-spin" />
            </Button>
          </div>
        </div>
      </div>

      <!-- Context Input -->
      <div class="space-y-2">
        <Label for="context">风格控制 (可选)</Label>
        <Input
          id="context"
          v-model="form.context"
          placeholder="例如：用温柔的语气，语速稍慢"
        />
      </div>

      <!-- Submit Button -->
      <Button
        @click="handleSubmit"
        :disabled="isSubmitting || !form.text.trim() || !form.voice || !configStore.apiKey"
        class="w-full sm:w-auto text-sm sm:text-base"
      >
        {{ isSubmitting ? '合成中...' : '开始合成' }}
      </Button>
    </CardContent>
  </Card>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { toast } from 'vue-sonner'
import { useTaskStore } from '@/stores/task'
import { useConfigStore } from '@/stores/config'
import { api, type Voice } from '@/api/client'
import { Card, CardHeader, CardTitle, CardDescription } from '@/components/ui/card'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { 
  Play as PlayIcon, 
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

const form = ref({
  text: '',
  voice: '',
  model: 'mimo-v2.5-tts',
  context: '',
})

const charCount = ref(0)
const estimatedTokens = ref(0)

function updateCounts() {
  charCount.value = form.value.text.length
  const chineseChars = (form.value.text.match(/[\u4e00-\u9fff]/g) || []).length
  const englishWords = form.value.text.split(/\s+/).filter(w => w).length
  estimatedTokens.value = Math.ceil(chineseChars * 1.5 + englishWords * 0.75)
}

async function loadVoices() {
  voicesLoading.value = true
  try {
    voices.value = await api.getVoices()
    if (voices.value.length > 0 && !form.value.voice) {
      form.value.voice = voices.value[0].id
    }
  } catch (error) {
    toast.error('加载音色列表失败')
    console.error(error)
  } finally {
    voicesLoading.value = false
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
  if (!configStore.apiKey) {
    toast.error('请配置 API Key')
    return
  }

  isSubmitting.value = true
  try {
    await taskStore.createTask({
      text: form.value.text,
      voice: form.value.voice,
      model: form.value.model,
      context: form.value.context || undefined,
      api_key: configStore.apiKey,
    })
    
    toast.success('任务创建成功')
    form.value.text = ''
    form.value.context = ''
    updateCounts()
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

  // 如果正在播放同一个音色，停止播放
  if (previewingVoice.value === voiceId && currentAudio.value) {
    currentAudio.value.pause()
    currentAudio.value = null
    previewingVoice.value = null
    return
  }

  // 停止之前的播放
  if (currentAudio.value) {
    currentAudio.value.pause()
    currentAudio.value = null
  }

  try {
    previewingVoice.value = voiceId
    
    // 优先使用 CDN URL，回退到后端代理
    const previewUrl = api.getVoicePreviewUrl(voiceId, voice.preview_url)
    console.log(`[音色预览] 使用 URL: ${previewUrl}`)
    
    const audio = new Audio(previewUrl)
    currentAudio.value = audio
    
    audio.onended = () => {
      console.log(`[音色预览] 播放完成: ${voiceId}`)
      previewingVoice.value = null
      currentAudio.value = null
    }
    
    audio.onerror = (e) => {
      console.error(`[音色预览] 加载失败:`, e, 'URL:', previewUrl)
      toast.error('试听音频加载失败')
      previewingVoice.value = null
      currentAudio.value = null
    }
    
    await audio.play()
    console.log(`[音色预览] 开始播放: ${voiceId}`)
  } catch (error) {
    console.error(`[音色预览] 播放异常:`, error)
    toast.error('试听播放失败')
    previewingVoice.value = null
    currentAudio.value = null
  }
}

onMounted(() => {
  loadVoices()
})
</script>
