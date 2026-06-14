<template>
  <slot v-if="!error" />
  <div v-else class="p-8 text-center">
    <AlertCircle class="w-12 h-12 text-destructive mx-auto mb-4" />
    <p class="text-destructive font-medium mb-2">{{ error }}</p>
    <Button size="sm" @click="reset">重试</Button>
  </div>
</template>

<script setup lang="ts">
import { ref, onErrorCaptured } from 'vue'
import { AlertCircle } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'

const error = ref<string | null>(null)

onErrorCaptured((e: Error) => {
  error.value = e.message || '组件错误'
  return false
})

function reset() { error.value = null }
</script>
