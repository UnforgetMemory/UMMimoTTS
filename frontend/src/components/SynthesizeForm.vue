<template>
  <div class="glass-card rounded-2xl overflow-hidden">
    <!-- 顶部标签页切换 -->
    <div class="flex items-center gap-1 bg-muted/50 p-1.5 border-b border-border/40">
      <button
        class="flex-1 py-2 px-3 rounded-lg text-sm font-medium transition-all duration-200"
        :class="activeTab === 'control' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'"
        @click="activeTab = 'control'">
        <Sparkles class="w-4 h-4 inline mr-1.5 -mt-0.5" />
        控制
      </button>
      <button
        class="flex-1 py-2 px-3 rounded-lg text-sm font-medium transition-all duration-200"
        :class="activeTab === 'config' ? 'bg-background shadow-sm text-foreground' : 'text-muted-foreground hover:text-foreground'"
        @click="activeTab = 'config'">
        <Settings class="w-4 h-4 inline mr-1.5 -mt-0.5" />
        配置
      </button>
    </div>

    <!-- 控制标签页 -->
    <div v-if="activeTab === 'control'" class="p-5 sm:p-6 space-y-5">
      <!-- 徽章区：模型 + 音色 + Provider -->
      <div class="flex items-center gap-2 flex-wrap">
        <Badge variant="secondary" class="text-xs gap-1 border border-violet-200 dark:border-violet-800 bg-violet-50 dark:bg-violet-950/30 text-violet-700 dark:text-violet-300">
          <Sparkles class="w-3 h-3" />
          {{ configStore.selectedModel }}
        </Badge>
        <Badge v-if="selectedVoiceObj" variant="outline" class="text-xs gap-1"
               :class="isMaleVoice ? 'border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-950/30 text-blue-700 dark:text-blue-300' : 'border-pink-200 dark:border-pink-800 bg-pink-50 dark:bg-pink-950/30 text-pink-700 dark:text-pink-300'">
          <UserRound v-if="isMaleVoice" class="w-3 h-3" />
          <User v-else class="w-3 h-3" />
          {{ selectedVoiceObj.name }}
        </Badge>
        <Badge v-else variant="destructive" class="text-xs">请选择音色</Badge>
        <Badge v-if="selectedProviderObj" variant="outline" class="text-xs gap-1 border-amber-200 dark:border-amber-800 bg-amber-50 dark:bg-amber-950/30 text-amber-700 dark:text-amber-300">
          <Server class="w-3 h-3" />
          {{ selectedProviderObj.name }}
          <Badge v-if="selectedProviderObj.is_default" class="text-[8px] h-3.5 px-1 ml-0.5" variant="default">默认</Badge>
        </Badge>
      </div>

      <!-- 文本输入 -->
      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <Label for="text" class="text-sm font-medium">合成文本 <span class="text-destructive">*</span></Label>
          <span class="text-xs text-muted-foreground">{{ charCount.toLocaleString() }} 字符</span>
        </div>
        <Textarea id="text" v-model="text" placeholder="输入要合成的文本内容..." rows="5" class="resize-none" />
      </div>

      <!-- 音色选择 -->
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

      <!-- Provider 选择 (简化版) -->
      <div v-if="availableProviders.length > 0" class="space-y-2">
        <div class="flex items-center justify-between">
          <Label class="text-sm font-medium">供应商</Label>
          <RouterLink to="/settings" class="text-xs text-primary hover:underline">管理供应商</RouterLink>
        </div>
        <Select v-model="selectedProviderId">
          <SelectTrigger class="h-9">
            <SelectValue :placeholder="selectedProviderObj?.name || '选择供应商'" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem v-for="p in availableProviders" :key="p.id" :value="p.id">
              <span class="flex items-center gap-2">
                <span class="w-2 h-2 rounded-full" :class="p.is_configured ? 'bg-green-500' : 'bg-gray-300'" />
                {{ p.name }}
                <span v-if="p.is_default" class="text-[10px] text-muted-foreground ml-1">(默认)</span>
              </span>
            </SelectItem>
          </SelectContent>
        </Select>
      </div>
      <div v-else class="rounded-lg border border-dashed border-border p-3 text-center">
        <p class="text-xs text-muted-foreground">未配置 Provider</p>
        <RouterLink to="/settings" class="text-xs text-primary hover:underline mt-1 inline-block">前往配置 →</RouterLink>
      </div>

      <!-- 风格控制 -->
      <div class="space-y-2">
        <Label for="context" class="text-sm font-medium">风格控制 <span class="text-muted-foreground font-normal">(可选)</span></Label>
        <Textarea id="context" v-model="context" placeholder="例如：用温柔的语气，语速稍慢" rows="2" maxlength="1024" class="resize-none" />
      </div>

      <Button @click="handleSubmit" :disabled="!canSubmit || isSubmitting || !text.trim() || !selectedVoice" class="w-full h-10">
        <Loader2 v-if="isSubmitting" class="w-4 h-4 mr-2 animate-spin" />
        {{ isSubmitting ? '合成中...' : '开始合成' }}
      </Button>
      <p v-if="!availableProviders.length" class="text-xs text-center text-muted-foreground">请先配置 Provider API Key</p>
    </div>

    <!-- 配置标签页 — 简化版音色/模型设置 -->
    <div v-else class="p-5 sm:p-6 space-y-5">
      <p class="text-sm text-muted-foreground">配置页面正在开发中...</p>
      <div class="space-y-3">
        <div class="flex items-center justify-between p-3 rounded-lg border border-border/60">
          <div>
            <p class="text-sm font-medium">可用音色</p>
            <p class="text-xs text-muted-foreground">{{ voices.length }} 个音色</p>
          </div>
          <Badge variant="secondary">{{ voices.length }}</Badge>
        </div>
        <div class="flex items-center justify-between p-3 rounded-lg border border-border/60">
          <div>
            <p class="text-sm font-medium">已配置 Provider</p>
            <p class="text-xs text-muted-foreground">{{ availableProviders.length }} 个</p>
          </div>
          <Badge :variant="availableProviders.length > 0 ? 'default' : 'destructive'">{{ availableProviders.length }}</Badge>
        </div>
      </div>
      <RouterLink to="/settings" class="block">
        <Button variant="outline" class="w-full">前往完整设置 →</Button>
      </RouterLink>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { RouterLink } from 'vue-router'
import { toast } from 'vue-sonner'
import { Sparkles, Settings, Server, User, UserRound, Loader2 } from 'lucide-vue-next'
import { useConfigStore } from '@/stores/config'
import { useTaskStore } from '@/stores/task'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Badge } from '@/components/ui/badge'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Button } from '@/components/ui/button'
import type { VoicePreset, ProviderInfo } from '@/types/config'

const configStore = useConfigStore()
const taskStore = useTaskStore()

const activeTab = ref<'control' | 'config'>('control')
const text = ref('')
const context = ref('')
const selectedVoice = ref('')
const selectedProviderId = ref('')
const isSubmitting = ref(false)

const voices = computed(() => configStore.voices)
const availableProviders = computed(() => configStore.providers.filter((p: ProviderInfo) => p.is_configured))
const selectedVoiceObj = computed(() => voices.value.find((v: VoicePreset) => v.id === selectedVoice.value))
const selectedProviderObj = computed(() => configStore.providers.find((p: ProviderInfo) => p.id === selectedProviderId.value))
const isMaleVoice = computed(() => selectedVoiceObj.value?.gender === '男性' || selectedVoiceObj.value?.gender === 'Male')
const canSubmit = computed(() => selectedProviderObj.value && text.value.trim() && selectedVoice.value)
const charCount = computed(() => text.value.length)

async function handleSubmit() {
  if (!text.value.trim() || !selectedVoice.value || !selectedProviderId.value) return
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
