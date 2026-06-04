<template>
  <Dialog :open="open" @update:open="$emit('update:open', $event)">
    <DialogContent class="sm:max-w-xl w-[95vw] xs:w-[90vw] sm:w-auto mx-auto p-6">
      <DialogHeader>
        <DialogTitle class="text-base sm:text-lg">API 配置</DialogTitle>
        <DialogDescription class="text-xs sm:text-sm">
          配置 API Key 和供应商以使用语音合成服务
        </DialogDescription>
      </DialogHeader>

      <div class="space-y-5 py-2 max-h-[70vh] overflow-y-auto pr-1">
        <!-- Hidden username field for Chrome password accessibility -->
        <input type="text" autocomplete="username" class="hidden" aria-hidden="true" tabindex="-1" />

        <!-- ── Global API Key ────────────────────────────────────── -->
        <div class="space-y-3">
          <h3 class="text-sm font-semibold">全局 API Key</h3>
          <div class="space-y-2">
            <Label for="api-key" class="text-sm">API Key</Label>
            <div class="flex gap-2">
              <Input
                id="api-key"
                v-model="apiKeyInput"
                type="password"
                autocomplete="new-password"
                placeholder="输入 MIMO API Key"
                class="text-sm flex-1 focus-visible:ring-2 focus-visible:ring-primary/50"
              />
              <Button @click="handleSaveGlobalKey" :disabled="!apiKeyInput.trim()" class="text-sm shrink-0">
                保存
              </Button>
            </div>
            <p class="text-xs text-muted-foreground">
              API Key 将保存在浏览器本地存储中。各供应商可单独配置 API Key。
            </p>
          </div>

          <!-- Global Key Status -->
          <div v-if="configStore.apiKey" class="p-3 rounded-lg border transition-colors"
               :class="configStore.hasValidKey
                 ? (isDark ? 'bg-green-950/30 border-green-800 text-green-300' : 'bg-green-50 border-green-200 text-green-700')
                 : (isDark ? 'bg-yellow-950/30 border-yellow-800 text-yellow-300' : 'bg-yellow-50 border-yellow-200 text-yellow-700')">
            <div class="flex items-center gap-2 text-sm">
              <template v-if="configStore.hasValidKey">
                <CheckIcon class="w-4 h-4 shrink-0" />
                <span>API Key 已配置</span>
              </template>
              <template v-else>
                <AlertTriangleIcon class="w-4 h-4 shrink-0" />
                <span>API Key 为环境占位符，请替换为真实 Key</span>
              </template>
            </div>
          </div>
        </div>

        <!-- Separator -->
        <div class="border-t border-border"></div>

        <!-- ── Per-Provider Configuration ────────────────────────── -->
        <div class="space-y-3">
          <div class="flex items-center justify-between">
            <h3 class="text-sm font-semibold">供应商配置</h3>
            <Button variant="ghost" size="sm" class="h-7 text-xs" @click="refreshProviders" :disabled="providersLoading">
              <Loader2Icon v-if="providersLoading" class="w-3 h-3 mr-1 animate-spin" />
              {{ providersLoading ? '加载中...' : '刷新' }}
            </Button>
          </div>

          <div v-if="providers.length === 0 && !providersLoading" class="text-xs text-muted-foreground text-center py-4">
            暂无供应商数据
          </div>

          <div v-for="provider in providers" :key="provider.id"
               class="rounded-lg border border-border overflow-hidden transition-colors"
               :class="[provider.is_default ? 'ring-1 ring-primary/30' : '']">
            <!-- Provider Header -->
            <div class="flex items-center justify-between px-3 py-2 bg-muted/50">
              <div class="flex items-center gap-2 min-w-0">
                <ServerIcon class="w-4 h-4 text-muted-foreground shrink-0" />
                <span class="text-sm font-medium truncate">{{ provider.name }}</span>
                <Badge v-if="provider.is_default" class="text-[10px] h-4 px-1.5" variant="default">默认</Badge>
                <Badge v-else-if="provider.is_configured" class="text-[10px] h-4 px-1.5" variant="secondary">已配置</Badge>
                <Badge v-else class="text-[10px] h-4 px-1.5" variant="outline">未配置</Badge>
              </div>
              <Button
                v-if="!provider.is_default && provider.is_configured"
                variant="outline"
                size="sm"
                class="h-6 text-xs shrink-0 ml-2"
                @click="handleSetDefault(provider.id)"
              >
                设为默认
              </Button>
            </div>
            <!-- Provider Body -->
            <div class="px-3 py-2 space-y-2">
              <div class="text-xs text-muted-foreground truncate font-mono">{{ provider.base_url }}</div>
              <div class="flex gap-2">
                <Input
                  :id="'pkey-' + provider.id"
                  v-model="providerKeys[provider.id]"
                  type="password"
                  autocomplete="new-password"
                  :placeholder="provider.is_configured ? '•••••••• (已配置，输入新值覆盖)' : '输入 API Key'"
                  class="text-sm flex-1 h-8 focus-visible:ring-2 focus-visible:ring-primary/50"
                />
                <Button
                  @click="handleSaveProviderKey(provider)"
                  :disabled="!(providerKeys[provider.id]?.trim())"
                  class="text-xs h-8 shrink-0"
                  size="sm"
                >
                  保存
                </Button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <DialogFooter class="flex-col sm:flex-row gap-2 sm:gap-0">
        <Button variant="outline" @click="handleClear" :disabled="!configStore.apiKey" class="w-full sm:w-auto text-sm">
          清除全局 Key
        </Button>
        <Button variant="outline" @click="$emit('update:open', false)" class="w-full sm:w-auto text-sm">
          关闭
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, reactive, watch, computed } from 'vue'
import { toast } from 'vue-sonner'
import { useConfigStore } from '@/stores/config'
import { useThemeStore } from '@/stores/theme'
import { apiV2, type ProviderInfo } from '@/api/client'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Label } from '@/components/ui/label'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Check as CheckIcon, AlertTriangle as AlertTriangleIcon, Loader2 as Loader2Icon, Server as ServerIcon } from 'lucide-vue-next'

interface Props {
  open: boolean
}

const props = defineProps<Props>()
const emit = defineEmits<{
  'update:open': [value: boolean]
}>()

const configStore = useConfigStore()
const themeStore = useThemeStore()
const isDark = computed(() => themeStore.actualTheme === 'dark')

// Global API key input
const apiKeyInput = ref('')

// Provider state
const providers = ref<ProviderInfo[]>([])
const providerKeys = reactive<Record<string, string>>({})
const providersLoading = ref(false)

// On dialog open, populate inputs
watch(() => props.open, async (isOpen) => {
  if (isOpen) {
    apiKeyInput.value = configStore.apiKey
    await refreshProviders()
  }
})

async function refreshProviders() {
  providersLoading.value = true
  try {
    providers.value = await apiV2.listProviders()
    // Seed providerKeys from current provider data (mask existing keys with empty)
    for (const p of providers.value) {
      if (!(p.id in providerKeys)) {
        providerKeys[p.id] = ''
      }
    }
  } catch (e: any) {
    toast.error('加载供应商失败: ' + (e.message || '未知错误'))
  } finally {
    providersLoading.value = false
  }
}

function handleSaveGlobalKey() {
  if (!apiKeyInput.value.trim()) {
    toast.error('请输入 API Key')
    return
  }
  configStore.saveApiKey(apiKeyInput.value.trim())
  toast.success('全局 API Key 已保存')
}

function handleClear() {
  configStore.clearApiKey()
  apiKeyInput.value = ''
  toast.success('全局 API Key 已清除')
}

async function handleSaveProviderKey(provider: ProviderInfo) {
  const key = providerKeys[provider.id]?.trim()
  if (!key) {
    toast.error(`请输入 ${provider.name} 的 API Key`)
    return
  }
  try {
    await apiV2.updateProviderKey(provider.id, key)
    toast.success(`${provider.name} API Key 已保存`)
    providerKeys[provider.id] = ''  // Clear input after save
    await refreshProviders()
  } catch (e: any) {
    toast.error(`保存 ${provider.name} 失败: ` + (e.message || '未知错误'))
  }
}

async function handleSetDefault(id: string) {
  try {
    await apiV2.setDefaultProvider(id)
    toast.success('默认供应商已更新')
    await refreshProviders()
    await configStore.loadProviders()
  } catch (e: any) {
    toast.error('设置默认供应商失败: ' + (e.message || '未知错误'))
  }
}
</script>
