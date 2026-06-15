<template>
  <div class="min-h-screen flex flex-col">
    <div class="flex-1">
      <div class="max-w-4xl mx-auto px-4 py-8 space-y-6">
        <header class="text-center pt-4 pb-2">
          <h1 class="text-3xl font-bold tracking-tight bg-gradient-to-r from-primary to-primary/60 bg-clip-text text-transparent">
            UM-MIMO-TTS
          </h1>
          <p class="text-sm text-muted-foreground mt-1">智能语音合成服务</p>
        </header>
        <SynthesizeForm />
        <TaskList />
      </div>
    </div>
    <Footer />
  </div>
</template>

<script setup lang="ts">
import SynthesizeForm from '@/components/SynthesizeForm.vue'
import TaskList from '@/components/TaskList.vue'
import Footer from '@/components/Footer.vue'
</script>
