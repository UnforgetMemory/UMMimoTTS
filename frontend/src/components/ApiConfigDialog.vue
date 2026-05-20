<template>
  <Dialog :open="open" @update:open="$emit('update:open', $event)">
    <DialogContent class="sm:max-w-md w-[95vw] xs:w-[90vw] sm:w-auto mx-auto">
      <DialogHeader>
        <DialogTitle class="text-base sm:text-lg">API 配置</DialogTitle>
        <DialogDescription class="text-xs sm:text-sm">
          配置 MIMO API Key 以使用语音合成服务
        </DialogDescription>
      </DialogHeader>

      <div class="space-y-4 py-4">
        <!-- API Key Input -->
        <div class="space-y-2">
          <Label for="api-key" class="text-sm">API Key</Label>
          <Input
            id="api-key"
            v-model="apiKey"
            type="password"
            placeholder="输入 MIMO API Key"
            class="text-sm"
          />
          <p class="text-xs text-muted-foreground">
            API Key 将保存在浏览器本地存储中
          </p>
        </div>

        <!-- Current Status -->
        <div v-if="configStore.apiKey" class="p-3 bg-green-50 dark:bg-green-950/20 rounded-lg border border-green-200 dark:border-green-900">
          <div class="flex items-center gap-2 text-sm text-green-700 dark:text-green-400">
            <CheckIcon class="w-4 h-4" />
            <span>API Key 已配置</span>
          </div>
        </div>
      </div>

      <DialogFooter class="flex-col sm:flex-row gap-2 sm:gap-0">
        <Button variant="outline" @click="handleClear" :disabled="!configStore.apiKey" class="w-full sm:w-auto text-sm">
          清除
        </Button>
        <Button @click="handleSave" :disabled="!apiKey" class="w-full sm:w-auto text-sm">
          保存
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { toast } from 'vue-sonner'
import { useConfigStore } from '@/stores/config'
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
import { Check as CheckIcon } from 'lucide-vue-next'

interface Props {
  open: boolean
}

defineProps<Props>()
defineEmits<{
  'update:open': [value: boolean]
}>()

const configStore = useConfigStore()
const apiKey = computed({
  get: () => configStore.apiKey,
  set: (value) => configStore.apiKey = value,
})

function handleSave() {
  if (!apiKey.value.trim()) {
    toast.error('请输入 API Key')
    return
  }
  configStore.saveApiKey(apiKey.value.trim())
  toast.success('API Key 已保存')
}

function handleClear() {
  configStore.clearApiKey()
  toast.success('API Key 已清除')
}
</script>
