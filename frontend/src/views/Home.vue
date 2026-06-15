<template>
  <div class="w-responsive px-3 sm:px-4 md:px-6 lg:px-8">
    <!-- 标签页按钮组 -->
    <div class="flex gap-1 p-1 bg-muted/50 rounded-xl mb-4">
      <button
        @click="activeTab = 'synthesize'"
        :class="['flex-1 flex items-center justify-center gap-1.5 py-2.5 px-4 rounded-lg text-sm font-medium transition-all duration-150',
                 activeTab === 'synthesize'
                   ? 'bg-background text-foreground shadow-sm'
                   : 'text-muted-foreground hover:text-foreground hover:bg-background/50']"
      >
        <Sparkles class="w-4 h-4" />
        合成
      </button>
      <button
        @click="activeTab = 'tasks'"
        :class="['flex-1 flex items-center justify-center gap-1.5 py-2.5 px-4 rounded-lg text-sm font-medium transition-all duration-150',
                 activeTab === 'tasks'
                   ? 'bg-background text-foreground shadow-sm'
                   : 'text-muted-foreground hover:text-foreground hover:bg-background/50']"
      >
        <ListIcon class="w-4 h-4" />
        任务
        <Badge v-if="taskCount > 0" variant="secondary" class="ml-1 text-[10px] h-4 px-1.5">
          {{ taskCount }}
        </Badge>
      </button>
    </div>

    <!-- 合成面板 (v-show 保持 DOM 避免重新挂载) -->
    <SynthesizeForm v-show="activeTab === 'synthesize'" />

    <!-- 任务列表 -->
    <TaskList v-show="activeTab === 'tasks'" />
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { Sparkles, List as ListIcon } from 'lucide-vue-next'
import { Badge } from '@/components/ui/badge'
import SynthesizeForm from '@/components/SynthesizeForm.vue'
import TaskList from '@/components/TaskList.vue'
import { useTaskStore } from '@/stores/task'

const taskStore = useTaskStore()
const activeTab = ref<'synthesize' | 'tasks'>('synthesize')
const taskCount = computed(() => taskStore.tasks.length)
</script>
