<template>
  <div class="max-w-4xl mx-auto px-4 py-8">
    <Tabs v-model="activeTab" class="space-y-4">
      <TabsList class="grid w-full grid-cols-2">
        <TabsTrigger value="synthesize">
          <Sparkles class="w-4 h-4 mr-1.5" />
          合成
        </TabsTrigger>
        <TabsTrigger value="tasks">
          <ListIcon class="w-4 h-4 mr-1.5" />
          任务
          <Badge v-if="taskStore.tasks.length > 0" variant="secondary" class="ml-1.5 text-[10px] h-4 px-1.5">
            {{ taskStore.tasks.length }}
          </Badge>
        </TabsTrigger>
      </TabsList>

      <!-- 合成面板 -->
      <TabsContent value="synthesize" class="mt-0" force-mount>
        <SynthesizeForm />
      </TabsContent>

      <!-- 任务列表 -->
      <TabsContent value="tasks" class="mt-0" force-mount>
        <TaskList />
      </TabsContent>
    </Tabs>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { Sparkles, List as ListIcon } from 'lucide-vue-next'
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs'
import { Badge } from '@/components/ui/badge'
import SynthesizeForm from '@/components/SynthesizeForm.vue'
import TaskList from '@/components/TaskList.vue'
import { useTaskStore } from '@/stores/task'

const taskStore = useTaskStore()
const activeTab = ref('synthesize')

// 切换到任务标签时自动刷新
watch(activeTab, (tab) => {
  if (tab === 'tasks') {
    taskStore.fetchTasks(0)
  }
})
</script>

<style scoped>
/* 强制 TabsContent 始终渲染，用 opacity 控制可见性避免切换卡顿 */
:deep([data-state="active"]) {
  display: block !important;
}
:deep([data-state="inactive"]) {
  display: none !important;
}
</style>
