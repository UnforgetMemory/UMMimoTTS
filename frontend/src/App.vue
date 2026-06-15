<template>
  <div :class="{ dark: uiStore.isDark }" class="min-h-screen bg-background text-foreground relative overflow-hidden">
    <!-- 背景装饰 -->
    <div class="absolute inset-0 pointer-events-none select-none overflow-hidden z-0" aria-hidden="true">
      <div class="bg-blob bg-blob-1"></div>
      <div class="bg-blob bg-blob-2"></div>
      <div class="bg-blob bg-blob-3"></div>
    </div>
    <!-- 品牌水印 -->
    <BrandHero />

    <!-- 主内容 -->
    <div class="relative z-10 flex flex-col min-h-screen">
      <RouterView />
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
