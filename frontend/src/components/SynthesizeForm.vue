<template>
  <Card>
    <CardHeader>
      <CardTitle>新建合成任务</CardTitle>
      <CardDescription>输入文本并选择音色进行语音合成</CardDescription>
    </CardHeader>
    <CardContent class="space-y-3 sm:space-y-4">
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
        
        <!-- 选择状态提示 -->
        <div v-if="form.voice" class="text-sm text-primary font-medium mb-2 flex items-center gap-2">
          <CheckIcon class="w-4 h-4" />
          已选择: {{ voices.find(v => v.id === form.voice)?.name }}
        </div>
        
        <div v-if="voicesLoading" class="text-sm text-muted-foreground">加载音色中...</div>
        <div v-else class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 2xl:grid-cols-6 gap-2 sm:gap-3">
          <Card
            v-for="voice in voices"
            :key="voice.id"
            class="cursor-pointer transition-all duration-150 relative group border-2"
            :class="{ 
              'border-primary bg-muted': form.voice === voice.id,
              'hover:border-primary/50': form.voice !== voice.id
            }"
            @click="form.voice = voice.id"
          >
            <CardContent class="p-3 sm:p-4">
              <!-- 选中标记 -->
              <div v-if="form.voice === voice.id" 
                   class="absolute top-2 right-2 w-5 h-5 rounded-full bg-primary flex items-center justify-center">
                <CheckIcon class="w-3 h-3 text-white" />
              </div>
              
              <!-- 音色信息 -->
              <div class="font-medium text-sm pr-6">{{ voice.name }}</div>
              <div class="text-xs text-muted-foreground mt-1">
                {{ voice.language }} · {{ voice.gender }}
              </div>
              
              <!-- 预览按钮（悬停显示） -->
              <Button
                size="sm"
                variant="ghost"
                class="absolute bottom-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity h-6 w-6 p-0"
                @click.stop="playVoicePreview(voice.id)"
                :disabled="previewingVoice === voice.id"
              >
                <PlayIcon v-if="previewingVoice !== voice.id" class="w-3 h-3" />
                <Loader2Icon v-else class="w-3 h-3 animate-spin" />
              </Button>
            </CardContent>
          </Card>
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
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from '@/components/ui/card'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Play as PlayIcon, Loader2 as Loader2Icon, Check as CheckIcon } from 'lucide-vue-next'

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
