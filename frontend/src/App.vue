<template>
  <div :class="{ dark: uiStore.isDark }" class="min-h-screen bg-background text-foreground">
    <RouterView />
  </div>
</template>

<script setup lang="ts">
import { useUIStore } from '@/stores/ui'

const uiStore = useUIStore()
</script>
