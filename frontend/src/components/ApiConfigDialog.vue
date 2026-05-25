<template>
  <Dialog :open="open" @update:open="$emit('update:open', $event)">
    <DialogContent class="sm:max-w-md w-[95vw] xs:w-[90vw] sm:w-auto mx-auto p-6">
      <DialogHeader>
        <DialogTitle class="text-base sm:text-lg">API 配置</DialogTitle>
        <DialogDescription class="text-xs sm:text-sm">
          配置 MIMO API Key 以使用语音合成服务
        </DialogDescription>
      </DialogHeader>

      <form @submit.prevent="handleSave">
      <div class="space-y-4 py-4">
        <!-- Hidden username field for Chrome password accessibility -->
        <input type="text" autocomplete="username" class="hidden" aria-hidden="true" tabindex="-1" />

        <!-- API Key Input -->
        <div class="space-y-2">
          <Label for="api-key" class="text-sm">API Key</Label>
          <Input
            id="api-key"
            v-model="apiKeyInput"
            type="password"
            autocomplete="new-password"
            placeholder="输入 MIMO API Key"
            class="text-sm focus-visible:ring-2 focus-visible:ring-primary/50"
          />
          <p class="text-xs text-muted-foreground">
            API Key 将保存在浏览器本地存储中
          </p>
        </div>

        <!-- Current Status -->
        <div v-if="configStore.apiKey" class="p-3 rounded-lg border transition-colors"
             :class="configStore.hasValidKey
               ? (isDark ? 'bg-green-950/30 border-green-800 text-green-300' : 'bg-green-50 border-green-200 text-green-700')
               : (isDark ? 'bg-yellow-950/30 border-yellow-800 text-yellow-300' : 'bg-yellow-50 border-yellow-200 text-yellow-700')">
          <div class="flex items-center gap-2 text-sm">
            <template v-if="configStore.hasValidKey">
              <CheckIcon class="w-4 h-4" />
              <span>API Key 已配置</span>
            </template>
            <template v-else>
              <AlertTriangleIcon class="w-4 h-4" />
              <span>API Key 为环境占位符，请替换为真实 Key</span>
            </template>
          </div>
        </div>
      </div>

      <DialogFooter class="flex-col sm:flex-row gap-2 sm:gap-0">
        <Button variant="outline" @click="handleClear" :disabled="!configStore.apiKey" class="w-full sm:w-auto text-sm">
          清除
        </Button>
        <Button @click="handleSave" :disabled="!apiKeyInput.trim()" class="w-full sm:w-auto text-sm">
          保存
        </Button>
      </DialogFooter>
      </form>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import { toast } from 'vue-sonner'
import { useConfigStore } from '@/stores/config'
import { useThemeStore } from '@/stores/theme'
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
import { Check as CheckIcon, AlertTriangle as AlertTriangleIcon } from 'lucide-vue-next'

interface Props {
  open: boolean
}

const props = defineProps<Props>()
defineEmits<{
  'update:open': [value: boolean]
}>()

const configStore = useConfigStore()
const themeStore = useThemeStore()
const isDark = computed(() => themeStore.actualTheme === 'dark')

// Local input value — NOT synced to store until save
const apiKeyInput = ref('')

// When dialog opens, populate input from store's current value
watch(() => props.open, (isOpen) => {
  if (isOpen) {
    apiKeyInput.value = configStore.apiKey
  }
})

function handleSave() {
  if (!apiKeyInput.value.trim()) {
    toast.error('请输入 API Key')
    return
  }
  configStore.saveApiKey(apiKeyInput.value.trim())
  toast.success('API Key 已保存')
}

function handleClear() {
  configStore.clearApiKey()
  apiKeyInput.value = ''
  toast.success('API Key 已清除')
}
</script>
