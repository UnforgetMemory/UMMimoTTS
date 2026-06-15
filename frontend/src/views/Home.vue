<template>
  <div class="max-w-3xl sm:max-w-4xl lg:max-w-5xl xl:max-w-6xl 2xl:max-w-7xl mx-auto px-3 sm:px-4 lg:px-6 py-4 sm:py-6 lg:py-8">
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

      <TabsContent value="synthesize" class="mt-0" force-mount>
        <SynthesizeForm />
      </TabsContent>

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

watch(activeTab, (tab) => {
  if (tab === 'tasks') taskStore.fetchTasks(0)
})
</script>
