<template>
  <div :class="{ dark: uiStore.isDark }" class="h-screen bg-background text-foreground flex flex-col">
    <!-- 背景装饰 -->
    <div class="fixed inset-0 pointer-events-none select-none overflow-hidden z-0" aria-hidden="true">
      <div class="bg-blob bg-blob-1"></div>
      <div class="bg-blob bg-blob-2"></div>
      <div class="bg-blob bg-blob-3"></div>
    </div>
    <!-- 品牌水印 -->
    <BrandHero />

    <!-- 主内容区域：flex-1 自动填充剩余空间，Footer 始终在底部 -->
    <div class="relative z-10 flex-1 overflow-y-auto">
      <RouterView />
    </div>

    <!-- Footer 固定在底部 -->
    <div class="relative z-10">
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
