<template>
  <div class="min-h-screen bg-background">
    <div class="max-w-4xl mx-auto px-4 py-8 space-y-8">
      <SynthesizeForm />
      <TaskList />
    </div>
  </div>
</template>

<script setup lang="ts">
import SynthesizeForm from '@/components/SynthesizeForm.vue'
import TaskList from '@/components/TaskList.vue'
</script>
