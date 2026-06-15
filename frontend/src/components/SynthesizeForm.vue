<template>
  <div class="glass-card rounded-xl sm:rounded-2xl overflow-hidden">
    <div class="p-3 sm:p-4 md:p-5 lg:p-6 space-y-4 sm:space-y-5">
      <!-- 加载骨架屏 -->
      <div v-if="!configStore.configLoaded" class="space-y-4 animate-pulse">
        <div class="flex gap-2"><div class="h-5 w-16 bg-muted rounded"></div><div class="h-5 w-12 bg-muted rounded"></div></div>
        <div class="h-20 bg-muted rounded-xl"></div>
        <div class="grid grid-cols-2 sm:grid-cols-4 gap-2"><div v-for="n in 8" :key="n" class="h-14 bg-muted rounded-xl"></div></div>
        <div class="h-9 bg-muted rounded-lg"></div>
        <div class="h-10 bg-muted rounded-lg"></div>
      </div>

      <template v-else>
        <!-- 徽章区 -->
        <div class="flex items-center gap-2 flex-wrap">
          <Badge variant="secondary" class="text-xs gap-1 border border-violet-200 dark:border-violet-800 bg-violet-50 dark:bg-violet-950/30 text-violet-700 dark:text-violet-300">
            <Sparkles class="w-3 h-3" />{{ configStore.selectedModel }}
          </Badge>
          <Badge v-if="selectedVoiceObj" variant="outline" class="text-xs gap-1"
                 :class="isMaleVoice ? 'border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-950/30 text-blue-700 dark:text-blue-300' : 'border-pink-200 dark:border-pink-800 bg-pink-50 dark:bg-pink-950/30 text-pink-700 dark:text-pink-300'">
            <UserRound v-if="isMaleVoice" class="w-3 h-3" /><User v-else class="w-3 h-3" />
            {{ selectedVoiceObj.name }}
          </Badge>
          <Badge v-else variant="destructive" class="text-xs">请选择音色</Badge>
          <Badge v-if="selectedProviderObj" variant="outline" class="text-xs gap-1 border-amber-200 dark:border-amber-800 bg-amber-50 dark:bg-amber-950/30 text-amber-700 dark:text-amber-300">
            <Server class="w-3 h-3" />{{ selectedProviderObj.name }}
          </Badge>
        </div>

        <!-- 文本输入 + 拖放导入 -->
        <div class="space-y-2">
          <div class="flex items-center justify-between">
            <Label for="text" class="text-sm font-medium">合成文本 <span class="text-destructive">*</span></Label>
            <div class="flex items-center gap-2">
              <span class="text-xs text-muted-foreground">{{ charCount.toLocaleString() }} 字符</span>
              <label class="text-xs text-primary hover:underline cursor-pointer">
                <input type="file" accept=".txt" class="hidden" @change="handleFileImport" />
                导入TXT
              </label>
            </div>
          </div>
          <div
            class="relative rounded-xl border transition-all duration-200"
            :class="[isDragOver ? 'border-primary bg-primary/5 ring-2 ring-primary/20' : 'border-border/60']"
            @dragover.prevent="isDragOver = true"
            @dragleave.prevent="isDragOver = false"
            @drop.prevent="handleDrop"
          >
            <textarea
              v-model="text"
              placeholder="输入要合成的文本内容，或拖入 .txt 文件..."
              class="w-full min-h-[120px] max-h-[400px] p-3 sm:p-4 text-sm leading-relaxed resize-y rounded-xl bg-transparent focus:outline-none focus:ring-0"
              @input="onTextInput"
              @mousedown="onResizeStart"
              style="field-sizing: content;"
            ></textarea>
            <!-- 拖放浮层 -->
            <Transition name="fade">
              <div v-if="isDragOver" class="absolute inset-0 flex items-center justify-center rounded-xl bg-primary/10 pointer-events-none">
                <div class="text-center">
                  <FileUp class="w-8 h-8 text-primary mx-auto mb-2" />
                  <p class="text-sm font-medium text-primary">释放以导入 TXT 文件</p>
                </div>
              </div>
            </Transition>
          </div>
        </div>

        <!-- 音色选择（带试听） -->
        <div class="space-y-2">
          <Label class="text-sm font-medium">音色 <span class="text-destructive">*</span></Label>
          <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-1.5 sm:gap-2">
            <div v-for="voice in voices" :key="voice.id"
                 class="group relative p-2 sm:p-3 rounded-lg sm:rounded-xl border text-left transition-all duration-200 cursor-pointer"
                 :class="selectedVoice === voice.id
                   ? 'border-primary bg-primary/5 ring-1 ring-primary/20'
                   : 'border-border/60 hover:border-primary/40 hover:bg-muted/30'"
                 @click="selectedVoice = voice.id">
              <div class="flex items-center justify-between">
                <div class="min-w-0">
                  <div class="font-medium text-sm">{{ voice.name }}</div>
                  <div class="text-[10px] text-muted-foreground/70 mt-0.5">{{ voice.style }}</div>
                </div>
                <!-- 试听按钮 -->
                <button
                  v-if="voice.preview_url"
                  class="shrink-0 w-7 h-7 rounded-full flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity bg-primary/10 hover:bg-primary/20"
                  @click.stop="playPreview(voice)"
                  :disabled="previewingVoice === voice.id"
                  :title="`试听 ${voice.name}`"
                >
                  <Loader2 v-if="previewingVoice === voice.id" class="w-3.5 h-3.5 animate-spin text-primary" />
                  <Play v-else class="w-3.5 h-3.5 text-primary" />
                </button>
                <!-- 选中标记 -->
                <Check v-if="selectedVoice === voice.id" class="w-4 h-4 text-primary shrink-0" />
              </div>
            </div>
          </div>
        </div>

        <!-- Provider 选择 + 内嵌配置 -->
        <div class="space-y-2">
          <div class="flex items-center justify-between">
            <Label class="text-sm font-medium">供应商</Label>
            <button @click="showProviderConfig = !showProviderConfig" class="text-xs text-muted-foreground hover:text-primary transition-colors">
              {{ showProviderConfig ? '收起配置' : (availableProviders.length ? '配置' : '配置 API Key') }}
            </button>
          </div>
          <div v-if="availableProviders.length > 0">
            <Select v-model="selectedProviderId">
              <SelectTrigger class="h-9"><SelectValue :placeholder="selectedProviderObj?.name || '选择供应商'" /></SelectTrigger>
              <SelectContent>
                <SelectItem v-for="p in availableProviders" :key="p.id" :value="p.id">
                  <span class="flex items-center gap-2"><span class="w-2 h-2 rounded-full bg-green-500" />{{ p.name }}</span>
                </SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div v-else class="rounded-lg border border-dashed border-border p-3 text-center"><p class="text-xs text-muted-foreground">未配置 Provider</p></div>
          <Transition name="slide">
            <div v-if="showProviderConfig" class="mt-3 space-y-3 rounded-xl border border-border/60 bg-muted/30 p-4">
              <div v-for="provider in allProviders" :key="provider.id" class="flex items-center gap-3">
                <div class="min-w-[120px] shrink-0">
                  <p class="text-xs font-medium">{{ provider.name }}</p>
                  <p class="text-[10px] text-muted-foreground/60 truncate">{{ provider.base_url }}</p>
                </div>
                <div class="flex items-center gap-1.5 flex-1">
                  <Badge v-if="provider.is_configured" class="text-[10px] shrink-0" variant="secondary">已配置</Badge>
                  <Badge v-else class="text-[10px] shrink-0" variant="outline">未配置</Badge>
                  <input :value="providerKeys[provider.id] || ''" @input="(e) => providerKeys[provider.id] = (e.target as HTMLInputElement).value" type="password" autocomplete="new-password" :placeholder="provider.is_configured ? '••••••••' : '输入 API Key'" class="flex-1 h-7 text-xs px-2 rounded-md border border-border/60 bg-background focus:outline-none focus:ring-1 focus:ring-primary/50" />
                  <Button size="sm" variant="ghost" class="h-7 px-2 text-[10px]" @click="saveProviderKey(provider)" :disabled="!providerKeys[provider.id]?.trim()">保存</Button>
                </div>
              </div>
            </div>
          </Transition>
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
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onUnmounted } from 'vue'
import { toast } from 'vue-sonner'
import { Sparkles, Server, User, UserRound, Loader2, Play, Check, FileUp } from 'lucide-vue-next'
import { useConfigStore } from '@/stores/config'
import { useTaskStore } from '@/stores/task'
import { configApi } from '@/api/config'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Badge } from '@/components/ui/badge'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Button } from '@/components/ui/button'
import type { VoicePreset, ProviderInfo } from '@/types/config'

const configStore = useConfigStore()
const taskStore = useTaskStore()

const text = ref('')
const context = ref('')
const selectedVoice = ref('')
const selectedProviderId = ref('')
const isSubmitting = ref(false)
const showProviderConfig = ref(false)
const providerKeys = ref<Record<string, string>>({})
const isDragOver = ref(false)
const previewingVoice = ref<string | null>(null)
let previewAudio: HTMLAudioElement | null = null

const voices = computed(() => configStore.voices)
const availableProviders = computed(() => configStore.providers.filter((p: ProviderInfo) => p.is_configured))
const allProviders = computed(() => configStore.providers)
const selectedVoiceObj = computed(() => voices.value.find((v: VoicePreset) => v.id === selectedVoice.value))
const selectedProviderObj = computed(() => configStore.providers.find((p: ProviderInfo) => p.id === selectedProviderId.value))
const isMaleVoice = computed(() => selectedVoiceObj.value?.gender === '男性' || selectedVoiceObj.value?.gender === 'Male')
const canSubmit = computed(() => selectedProviderObj.value && text.value.trim() && selectedVoice.value)
const charCount = computed(() => text.value.length)

// ── TXT 文件导入 ──────────────────────────
function handleFileImport(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  readTxtFile(file)
  ;(e.target as HTMLInputElement).value = ''
}

function handleDrop(e: DragEvent) {
  isDragOver.value = false
  const file = e.dataTransfer?.files?.[0]
  if (!file) return
  if (!file.name.endsWith('.txt')) { toast.error('仅支持 .txt 文件'); return }
  readTxtFile(file)
}

function readTxtFile(file: File) {
  if (file.size > 10 * 1024 * 1024) { toast.error('文件过大，最大支持 10MB'); return }
  const reader = new FileReader()
  reader.onload = (e) => {
    const content = e.target?.result as string
    text.value = content
    toast.success(`已导入 ${file.name} (${content.length.toLocaleString()} 字符)`)
  }
  reader.onerror = () => toast.error('文件读取失败')
  reader.readAsText(file, 'UTF-8')
}

function onTextInput() { /* textarea auto-resize handled by field-sizing: content */ }
function onResizeStart() { /* allow native resize */ }

// ── 音色试听 ──────────────────────────
function playPreview(voice: VoicePreset) {
  if (!voice.preview_url) return
  if (previewingVoice.value === voice.id) {
    previewAudio?.pause()
    previewAudio = null
    previewingVoice.value = null
    return
  }
  previewAudio?.pause()
  previewingVoice.value = voice.id
  previewAudio = new Audio(voice.preview_url)
  previewAudio.onended = () => { previewingVoice.value = null; previewAudio = null }
  previewAudio.onerror = () => { toast.error(`${voice.name} 试听失败`); previewingVoice.value = null; previewAudio = null }
  previewAudio.play().catch(() => { previewingVoice.value = null; previewAudio = null })
}

onUnmounted(() => { previewAudio?.pause(); previewAudio = null })

// ── 提交 ──────────────────────────
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
  } finally { isSubmitting.value = false }
}

async function saveProviderKey(provider: ProviderInfo) {
  const key = providerKeys.value[provider.id]?.trim()
  if (!key) { toast.error(`请输入 ${provider.name} 的 API Key`); return }
  try {
    await configApi.updateProviderKey(provider.id, key)
    toast.success(`${provider.name} API Key 已保存`)
    providerKeys.value[provider.id] = ''
    await configStore.loadProviders()
  } catch (e: any) { toast.error(`保存失败: ${e.message || '未知错误'}`) }
}
</script>

<style scoped>
.slide-enter-active, .slide-leave-active { transition: all 0.2s ease; }
.slide-enter-from, .slide-leave-to { opacity: 0; transform: translateY(-8px); }
.fade-enter-active, .fade-leave-active { transition: opacity 0.15s ease; }
.fade-enter-from, .fade-leave-to { opacity: 0; }
</style>
