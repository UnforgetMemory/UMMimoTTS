<template>
  <div class="h-full overflow-y-auto">
    <div class="max-w-3xl mx-auto p-6">
      <div class="flex items-center gap-3 mb-4">
        <Button variant="ghost" size="sm" @click="router.push('/tasks/single')">
          <ArrowLeftIcon class="w-4 h-4" />
        </Button>
      </div>
      <Card>
        <CardHeader>
          <CardTitle>API 配置</CardTitle>
          <CardDescription>配置 API Key 和供应商以使用语音合成服务</CardDescription>
        </CardHeader>
        <CardContent>
          <div class="space-y-5">
            <!-- Hidden username field for Chrome password accessibility -->
            <input type="text" autocomplete="username" class="hidden" aria-hidden="true" tabindex="-1" />

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

              <div class="flex flex-wrap gap-3">
                <div v-for="provider in providers" :key="provider.id"
                     class="rounded-lg border border-border overflow-hidden transition-colors flex-1 min-w-[280px]"
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
          </div>
        </CardContent>
      </Card>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { toast } from 'vue-sonner'
import { useConfigStore } from '@/stores/config'
import { apiV2, type ProviderInfo } from '@/api/client'
import { Card, CardHeader, CardContent, CardTitle, CardDescription } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Loader2 as Loader2Icon, Server as ServerIcon, ArrowLeft as ArrowLeftIcon } from 'lucide-vue-next'

const router = useRouter()
const configStore = useConfigStore()

// Provider state
const providers = ref<ProviderInfo[]>([])
const providerKeys = reactive<Record<string, string>>({})
const providersLoading = ref(false)

// On mount, populate inputs
onMounted(async () => {
  await refreshProviders()
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
    await configStore.loadProviders()
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
