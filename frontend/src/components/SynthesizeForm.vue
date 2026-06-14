<template>
  <Card>
    <CardHeader class="pb-3">
      <CardTitle>新建合成任务</CardTitle>
      <CardDescription>输入文本并选择音色进行语音合成</CardDescription>
    </CardHeader>
    <CardContent class="space-y-4">
      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <Label for="text">合成文本 <span class="text-destructive">*</span></Label>
          <span class="text-xs text-muted-foreground">{{ charCount }} 字符</span>
        </div>
        <Textarea id="text" v-model="text" placeholder="输入要合成的文本..." rows="5" />
      </div>
      <div class="space-y-2">
        <Label>音色 <span class="text-destructive">*</span></Label>
        <div class="grid grid-cols-2 sm:grid-cols-4 gap-2">
          <button v-for="voice in voices" :key="voice.id" @click="selectedVoice = voice.id"
                  :class="['p-3 rounded-lg border-2 text-sm text-left transition-all', selectedVoice === voice.id ? 'border-primary bg-primary/5' : 'border-border hover:border-primary/50']">
            <div class="font-medium">{{ voice.name }}</div>
            <div class="text-[10px] text-muted-foreground">{{ voice.style }}</div>
          </button>
        </div>
      </div>
      <div class="space-y-2">
        <Label for="context">风格控制 <span class="text-muted-foreground">(可选)</span></Label>
        <Textarea id="context" v-model="context" placeholder="例如：用温柔的语气，语速稍慢" rows="2" maxlength="1024" />
      </div>
      <Button @click="handleSubmit" :disabled="isSubmitting || !text.trim() || !selectedVoice || !canSubmit" class="w-full">
        {{ isSubmitting ? '合成中...' : '开始合成' }}
      </Button>
      <p v-if="!canSubmit" class="text-xs text-center text-muted-foreground">请先配置 Provider API Key</p>
    </CardContent>
  </Card>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { toast } from 'vue-sonner'
import { useConfigStore } from '@/stores/config'
import { useTaskStore } from '@/stores/task'
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from '@/components/ui/card'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Button } from '@/components/ui/button'

const configStore = useConfigStore()
const taskStore = useTaskStore()

const text = ref('')
const context = ref('')
const selectedVoice = ref('')
const isSubmitting = ref(false)

const voices = computed(() => configStore.voices)
const canSubmit = computed(() => configStore.hasConfiguredProvider || configStore.hasValidKey)
const charCount = computed(() => text.value.length)

async function handleSubmit() {
  if (!text.value.trim() || !selectedVoice.value) return
  isSubmitting.value = true
  try {
    await taskStore.createTask(text.value, selectedVoice.value, configStore.selectedModel, context.value || undefined)
    toast.success('任务创建成功')
    text.value = ''
    context.value = ''
    await taskStore.fetchTasks(0)
  } catch (e: any) {
    toast.error(e.message || '创建任务失败')
  } finally {
    isSubmitting.value = false
  }
}
</script>
