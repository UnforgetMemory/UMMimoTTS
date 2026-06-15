<template>
  <div class="w-responsive px-3 sm:px-4 lg:px-6 py-4 sm:py-6 lg:py-8">
    <div class="flex items-center gap-3 mb-4">
      <Button variant="ghost" size="sm" @click="router.push('/')">
        <ArrowLeft class="w-4 h-4" />
      </Button>
    </div>
    <div class="glass-card rounded-xl p-6">
      <div class="mb-6">
        <h1 class="text-xl font-semibold">API 配置</h1>
        <p class="text-sm text-muted-foreground mt-1">配置 API Key 和供应商以使用语音合成服务</p>
      </div>
      <div class="space-y-5">
        <input type="text" autocomplete="username" class="hidden" aria-hidden="true" tabindex="-1" />
        <div class="space-y-3">
          <div class="flex items-center justify-between">
            <h3 class="text-sm font-semibold">供应商配置</h3>
            <Button variant="ghost" size="sm" class="h-7 text-xs" @click="handleRefresh" :disabled="loading">
              <Loader2Icon v-if="loading" class="w-3 h-3 mr-1 animate-spin" />
              {{ loading ? '加载中...' : '刷新' }}
            </Button>
          </div>
          <div v-if="providers.length === 0 && !loading" class="text-xs text-muted-foreground text-center py-4">
            暂无供应商数据
          </div>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <div v-for="provider in providers" :key="provider.id"
                 class="rounded-lg border border-border/50 overflow-hidden transition-colors bg-card/50"
                 :class="[provider.is_default ? 'ring-1 ring-primary/30' : '']">
              <div class="flex items-center justify-between px-3 py-2 bg-muted/30">
                <div class="flex items-center gap-2 min-w-0">
                  <ServerIcon class="w-4 h-4 text-muted-foreground shrink-0" />
                  <span class="text-sm font-medium truncate">{{ provider.name }}</span>
                  <Badge v-if="provider.is_default" class="text-[10px] h-4 px-1.5" variant="default">默认</Badge>
                  <Badge v-else-if="provider.is_configured" class="text-[10px] h-4 px-1.5" variant="secondary">已配置</Badge>
                  <Badge v-else class="text-[10px] h-4 px-1.5" variant="outline">未配置</Badge>
                </div>
                <Button v-if="!provider.is_default && provider.is_configured" variant="outline" size="sm" class="h-6 text-xs shrink-0 ml-2" @click="handleSetDefault(provider.id)">设为默认</Button>
              </div>
              <div class="px-3 py-2 space-y-2">
                <div class="text-xs text-muted-foreground truncate font-mono">{{ provider.base_url }}</div>
                <div class="flex gap-2">
                  <Input :id="'pkey-' + provider.id" v-model="providerKeys[provider.id]" type="password" autocomplete="new-password"
                         :placeholder="provider.is_configured ? '•••••••• (已配置，输入新值覆盖)' : '输入 API Key'"
                         class="text-sm flex-1 h-8 focus-visible:ring-2 focus-visible:ring-primary/50" />
                  <Button @click="handleSaveProviderKey(provider)" :disabled="!(providerKeys[provider.id]?.trim())" class="text-xs h-8 shrink-0" size="sm">保存</Button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, onMounted, computed } from 'vue'
import { useRouter } from 'vue-router'
import { toast } from 'vue-sonner'
import { ArrowLeft } from 'lucide-vue-next'
import { Loader2 as Loader2Icon, Server as ServerIcon } from 'lucide-vue-next'
import { useConfigStore } from '@/stores/config'
import { configApi } from '@/api/config'
import type { ProviderInfo } from '@/types/config'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'

const router = useRouter()
const configStore = useConfigStore()
const providers = computed(() => configStore.providers)
const providerKeys = reactive<Record<string, string>>({})
const loading = ref(false)

onMounted(async () => { await handleRefresh() })

async function handleRefresh() {
  loading.value = true
  try {
    await configStore.loadProviders()
    for (const p of providers.value) {
      if (!(p.id in providerKeys)) providerKeys[p.id] = ''
    }
  } catch (e: any) {
    toast.error('加载供应商失败: ' + (e.message || '未知错误'))
  } finally {
    loading.value = false
  }
}

async function handleSaveProviderKey(provider: ProviderInfo) {
  const key = providerKeys[provider.id]?.trim()
  if (!key) { toast.error(`请输入 ${provider.name} 的 API Key`); return }
  try {
    await configApi.updateProviderKey(provider.id, key)
    toast.success(`${provider.name} API Key 已保存`)
    providerKeys[provider.id] = ''
    await configStore.loadProviders()
  } catch (e: any) {
    toast.error(`保存 ${provider.name} 失败: ` + (e.message || '未知错误'))
  }
}

async function handleSetDefault(id: string) {
  try {
    await configApi.setDefaultProvider(id)
    toast.success('默认供应商已更新')
    await configStore.loadProviders()
  } catch (e: any) {
    toast.error('设置默认供应商失败: ' + (e.message || '未知错误'))
  }
}
</script>
