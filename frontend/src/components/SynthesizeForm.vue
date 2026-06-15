<template>
  <div class="glass-card rounded-2xl p-5 sm:p-6 space-y-5">
    <div class="flex items-center gap-2 mb-1">
      <Sparkles class="w-5 h-5 text-primary" />
      <h2 class="text-lg font-semibold">新建合成任务</h2>
    </div>

    <!-- Provider 选择 -->
    <div v-if="availableProviders.length > 0" class="space-y-2">
      <div class="flex items-center justify-between">
        <Label class="text-sm font-medium">供应商</Label>
        <RouterLink to="/settings" class="text-xs text-primary hover:underline">配置供应商</RouterLink>
      </div>
      <Select v-model="selectedProviderId">
        <SelectTrigger class="h-9">
          <SelectValue :placeholder="selectedProvider?.name || '选择供应商'" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem v-for="p in availableProviders" :key="p.id" :value="p.id">
            <span class="flex items-center gap-2">
              <span class="w-2 h-2 rounded-full bg-green-500" v-if="p.is_configured" />
              <span class="w-2 h-2 rounded-full bg-gray-300" v-else />
              {{ p.name }}
            </span>
          </SelectItem>
        </SelectContent>
      </Select>
    </div>
    <div v-else class="rounded-lg border border-dashed border-border p-3 text-center">
      <p class="text-xs text-muted-foreground">未配置 Provider</p>
      <RouterLink to="/settings" class="text-xs text-primary hover:underline mt-1 inline-block">前往配置 →</RouterLink>
    </div>

    <div class="space-y-2">
      <div class="flex items-center justify-between">
        <Label for="text" class="text-sm font-medium">合成文本 <span class="text-destructive">*</span></Label>
        <span class="text-xs text-muted-foreground">{{ charCount.toLocaleString() }} 字符</span>
      </div>
      <Textarea id="text" v-model="text" placeholder="输入要合成的文本内容..." rows="5" class="resize-none" />
    </div>

    <div class="space-y-2">
      <Label class="text-sm font-medium">音色 <span class="text-destructive">*</span></Label>
      <div class="grid grid-cols-2 sm:grid-cols-4 gap-2">
        <button v-for="voice in voices" :key="voice.id" @click="selectedVoice = voice.id"
                :class="['p-3 rounded-xl border text-left transition-all duration-200',
                         selectedVoice === voice.id
                           ? 'border-primary bg-primary/5 ring-1 ring-primary/20'
                           : 'border-border/60 hover:border-primary/40 hover:bg-muted/30']">
          <div class="font-medium text-sm">{{ voice.name }}</div>
          <div class="text-[10px] text-muted-foreground/70 mt-0.5">{{ voice.style }}</div>
        </button>
      </div>
    </div>

    <div class="space-y-2">
      <Label for="context" class="text-sm font-medium">风格控制 <span class="text-muted-foreground font-normal">(可选)</span></Label>
      <Textarea id="context" v-model="context" placeholder="例如：用温柔的语气，语速稍慢" rows="2" maxlength="1024" class="resize-none" />
    </div>

    <Button @click="handleSubmit" :disabled="!canSubmit || isSubmitting || !text.trim() || !selectedVoice" class="w-full h-10">
      <Loader2 v-if="isSubmitting" class="w-4 h-4 mr-2 animate-spin" />
      {{ isSubmitting ? '合成中...' : '开始合成' }}
    </Button>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { RouterLink } from 'vue-router'
import { toast } from 'vue-sonner'
import { Sparkles, Loader2 } from 'lucide-vue-next'
import { useConfigStore } from '@/stores/config'
import { useTaskStore } from '@/stores/task'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Button } from '@/components/ui/button'

const configStore = useConfigStore()
const taskStore = useTaskStore()

const text = ref('')
const context = ref('')
const selectedVoice = ref('')
const selectedProviderId = ref('')
const isSubmitting = ref(false)

const voices = computed(() => configStore.voices)
const availableProviders = computed(() => configStore.providers.filter((p: any) => p.is_configured))
const selectedProvider = computed(() => configStore.providers.find((p: any) => p.id === selectedProviderId.value))
const canSubmit = computed(() => selectedProvider.value && (text.value.trim() && selectedVoice.value))
const charCount = computed(() => text.value.length)

async function handleSubmit() {
  if (!text.value.trim() || !selectedVoice.value || !selectedProviderId.value) return
  isSubmitting.value = true
  try {
    await taskStore.createTask(text.value, selectedVoice.value, configStore.selectedModel, context.value || undefined, selectedProviderId.value)
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
