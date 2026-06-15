<template>
  <div :class="{ dark: uiStore.isDark }" class="h-screen bg-background text-foreground flex flex-col">
    <!-- 背景装饰 blobs (z-0) -->
    <div class="fixed inset-0 pointer-events-none select-none overflow-hidden" style="z-index: 0;" aria-hidden="true">
      <div class="bg-blob bg-blob-1"></div>
      <div class="bg-blob bg-blob-2"></div>
      <div class="bg-blob bg-blob-3"></div>
    </div>
    <!-- 品牌水印 (z-1, 在 blobs 上方) -->
    <BrandHero />

    <!-- 主内容区域 (z-10, 最顶层) -->
    <div class="relative flex-1 overflow-y-auto" style="z-index: 10;">
      <RouterView />
    </div>

    <!-- Footer (z-10) -->
    <div class="relative" style="z-index: 10;">
      <Footer />
    </div>
  </div>
</template>

<script setup lang="ts">
import { useUIStore } from '@/stores/ui'
import BrandHero from '@/components/BrandHero.vue'
import Footer from '@/components/Footer.vue'

const uiStore = useUIStore()
</script>
