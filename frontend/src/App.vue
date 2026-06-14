<template>
  <div class="min-h-screen bg-background relative">
    <!-- 背景水印 -->
    <BrandHero />
    
    <!-- 主内容区 -->
    <div class="relative z-1 flex min-h-screen">
      <!-- 左侧批量任务面板 (覆盖式，不占位) -->
      <Transition name="slide-in-left">
        <aside
          v-if="!sidebarCollapsed"
          class="fixed left-0 top-0 h-full bg-background text-foreground border-r shadow-lg z-50 flex flex-col
                 w-full xs:w-80 sm:w-96 md:w-[28rem] lg:w-[30rem]"
          role="complementary"
          aria-label="批量任务面板"
          aria-modal="true"
          tabindex="-1"
        >
          <!-- Header -->
          <div class="px-4 sm:px-5 py-3.5 border-b shrink-0">
            <div class="flex items-center justify-between">
              <div class="min-w-0 flex-1">
                <h2 class="text-base sm:text-lg font-semibold tracking-tight text-foreground">批量任务</h2>
                <p class="text-xs text-muted-foreground/70 mt-0.5">管理批量合成分组</p>
              </div>
              <Button 
                variant="ghost" 
                size="sm" 
                class="h-7 w-7 sm:h-8 sm:w-8 p-0 shrink-0 ml-2 text-muted-foreground hover:text-foreground"
                @click="sidebarCollapsed = true"
              >
                <XIcon class="w-3.5 h-3.5 sm:w-4 sm:h-4" />
              </Button>
            </div>
          </div>
          
          <!-- New task button -->
          <div class="px-4 sm:px-5 py-3 border-b">
            <Button
              class="w-full h-9 text-xs"
              size="sm"
              :disabled="!configStore.hasValidKey"
              @click="showBatchWizard = true"
            >
              <PlusIcon class="w-4 h-4 mr-1.5" />
              {{ configStore.hasValidKey ? '新建批量任务' : '请先配置 API Key' }}
            </Button>
          </div>

          <!-- Group count + clear all -->
          <div class="flex items-center justify-between px-4 sm:px-5 py-2 border-b bg-muted/10">
            <span class="text-xs font-medium text-muted-foreground">
              {{ batchStore.groups.length }} 个分组
            </span>
            <Button
              v-if="batchStore.groups.length > 0"
              variant="ghost"
              size="sm"
              class="h-7 text-xs text-muted-foreground hover:text-destructive px-2 -mr-1"
              @click="handleClearAll"
            >
              <Trash2Icon class="w-3.5 h-3.5 mr-1" />
              一键清空
            </Button>
          </div>

          <!-- Group list -->
          <div class="flex-1 overflow-hidden">
            <GroupKanban
              :groups="batchStore.groups"
              :selected-group-id="selectedGroupId"
              :loading="batchStore.loading"
              @select="router.push(`/groups/${$event}`)"
              @pause="handlePauseGroup"
              @resume="handleResumeGroup"
              @retry="handleRetryGroup"
              @cancel="handleCancelGroup"
              @delete="handleDeleteGroup"
              @download="handleDownloadGroup"
              class="h-full"
            />
          </div>
        </aside>
      </Transition>

      <!-- Overlay -->
      <Transition name="fade">
        <div 
          v-if="!sidebarCollapsed"
          class="fixed inset-0 bg-black/15 z-40"
          @click="sidebarCollapsed = true"
        />
      </Transition>

      <!-- Center content area with router -->
      <main class="flex-1 flex flex-col items-center justify-start px-4 pt-20 sm:pt-24 pb-6 sm:pb-10 overflow-y-auto scrollbar-auto">
        <router-view v-slot="{ Component: RouteComponent }">
          <Transition name="page-slide" mode="out-in">
            <component :is="RouteComponent" />
          </Transition>
        </router-view>
        
        <!-- Footer -->
        <FooterInfo />
      </main>
    </div>

    <!-- Floating toolbar -->
    <FloatingToolbar class="z-50" />

    <!-- Mobile batch sidebar -->
    <Transition name="slide-in-left">
      <aside
        v-if="showMobileBatchSidebar"
        class="fixed left-0 top-0 h-full w-72 bg-background border-r shadow-lg z-50 flex flex-col lg:hidden"
      >
        <div class="px-4 py-3.5 border-b flex items-center justify-between">
          <h2 class="text-sm font-semibold text-foreground">批量任务</h2>
          <Button
            variant="ghost"
            size="sm"
            class="h-7 w-7 p-0 text-muted-foreground hover:text-foreground"
            @click="showMobileBatchSidebar = false"
          >
            <XIcon class="w-4 h-4" />
          </Button>
        </div>
        <div class="flex-1 overflow-y-auto scrollbar-auto p-3 space-y-2">
          <Button
            class="w-full h-9 text-xs"
            size="sm"
            :disabled="!configStore.hasValidKey"
            @click="showMobileBatchSidebar = false; showBatchWizard = true"
          >
            <PlusIcon class="w-4 h-4 mr-1.5" />
            {{ configStore.hasValidKey ? '新建批量任务' : '请先配置 API Key' }}
          </Button>
          <GroupCard
            v-for="group in batchStore.groups"
            :key="group.id"
            :group="group"
            :selected="selectedGroupId === group.id"
            @select="showMobileBatchSidebar = false; router.push(`/groups/${$event}`)"
            @pause="handlePauseGroup"
            @resume="handleResumeGroup"
            @retry="handleRetryGroup"
            @cancel="handleCancelGroup"
            @delete="handleDeleteGroup"
          />
        </div>
      </aside>
    </Transition>

    <Transition name="fade">
      <div
        v-if="showMobileBatchSidebar"
        class="fixed inset-0 bg-black/15 z-40 lg:hidden"
        @click="showMobileBatchSidebar = false"
        aria-hidden="true"
      ></div>
    </Transition>

    <BatchImportWizard
      v-model:open="showBatchWizard"
      @imported="handleBatchImported"
    />

    <Toaster />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, provide } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useTaskStore } from '@/stores/task'
import { useBatchStore } from '@/stores/batch'
import { useConfigStore } from '@/stores/config'
import { toast } from 'vue-sonner'
import BrandHero from './components/BrandHero.vue'
import FloatingToolbar from './components/FloatingToolbar.vue'
import FooterInfo from './components/FooterInfo.vue'
import BatchImportWizard from './components/BatchImportWizard.vue'
import GroupKanban from './components/GroupKanban.vue'
import GroupCard from './components/GroupCard.vue'
import { Toaster } from '@/components/ui/sonner'
import { Button } from '@/components/ui/button'
import { 
  X as XIcon, 
  Plus as PlusIcon,
  Trash2 as Trash2Icon,
} from 'lucide-vue-next'

const router = useRouter()
const route = useRoute()
const taskStore = useTaskStore()
const batchStore = useBatchStore()
const configStore = useConfigStore()
const showBatchWizard = ref(false)
const showMobileBatchSidebar = ref(false)
const sidebarCollapsed = ref(true)  // 默认折叠
const selectedGroupId = computed(() => route.name === 'group-detail' ? (route.params.id as string) : null)

// Provide batch wizard opener for child components
provide('openBatchWizard', () => { showBatchWizard.value = true })

// Keyboard event - ESC to close sidebars
function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && !sidebarCollapsed.value) {
    sidebarCollapsed.value = true
  }
}

// Batch group handlers
async function handlePauseGroup(groupId: string) {
  try {
    await batchStore.pauseGroup(groupId)
    toast.success('分组已暂停')
  } catch (error) {
    toast.error('暂停失败')
  }
}

async function handleResumeGroup(groupId: string) {
  try {
    await batchStore.resumeGroup(groupId)
    toast.success('分组已恢复')
  } catch (error) {
    toast.error('恢复失败')
  }
}

async function handleRetryGroup(groupId: string) {
  try {
    await batchStore.retryFailed(groupId)
    toast.success('失败任务已重新排队')
  } catch (error) {
    toast.error('重试失败')
  }
}

async function handleDeleteGroup(groupId: string) {
  if (!confirm('确定删除此分组及其所有任务？')) return
  try {
    await batchStore.removeGroup(groupId)
    if (selectedGroupId.value === groupId) {
      router.push('/tasks/single')
    }
    toast.success('分组已删除')
  } catch (error) {
    toast.error('删除失败')
  }
}

async function handleCancelGroup(groupId: string) {
  try {
    await batchStore.cancelGroup(groupId)
    toast.success('分组已停止')
  } catch (error) {
    toast.error('停止失败')
  }
}

async function handleClearAll() {
  const count = batchStore.groups.length
  if (!confirm(`确定要清空全部 ${count} 个分组吗？此操作不可恢复。`)) return
  try {
    await batchStore.clearAll()
    router.push('/tasks/single')
    toast.success('已清空全部分组')
  } catch (error) {
    toast.error('清空失败')
  }
}

async function handleDownloadGroup(groupId: string) {
  try {
    await batchStore.downloadGroupAudio(groupId)
    toast.success('下载已开始')
  } catch (error) {
    toast.error('下载失败')
  }
}

async function handleBatchImported(groupId: string) {
  showBatchWizard.value = false
  await Promise.all([
    batchStore.loadGroups(),
    taskStore.loadTasks(),
  ])
  router.push(`/groups/${groupId}`)
}

// Lifecycle
onMounted(async () => {
  document.addEventListener('keydown', handleKeydown)
  // init() calls loadTasks() + restoreSseSubscriptions() + startPolling() (30s fallback)
  taskStore.init()
  await batchStore.loadGroups()
})

onUnmounted(() => {
  document.removeEventListener('keydown', handleKeydown)
})
</script>

<style>
.slide-in-right-enter-active,
.slide-in-right-leave-active {
  transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}
.slide-in-right-enter-from,
.slide-in-right-leave-to {
  transform: translateX(100%);
}

.slide-in-left-enter-active,
.slide-in-left-leave-active {
  transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}
.slide-in-left-enter-from,
.slide-in-left-leave-to {
  transform: translateX(-100%);
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.page-slide-enter-active,
.page-slide-leave-active {
  transition: all 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}
.page-slide-enter-from {
  opacity: 0;
  transform: translateX(24px);
}
.page-slide-leave-to {
  opacity: 0;
  transform: translateX(-24px);
}
</style>
