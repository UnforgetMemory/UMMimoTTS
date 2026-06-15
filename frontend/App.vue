<template>
  <div class="min-h-screen bg-background relative overflow-hidden flex flex-col">
    <!-- 背景装饰 (hidden on very small screens) -->
    <div class="absolute inset-0 pointer-events-none select-none overflow-hidden">
      <div class="bg-blob absolute -top-32 -left-32 w-96 h-96 rounded-full bg-primary/[0.07] dark:bg-primary/[0.04] blur-3xl" />
      <div class="bg-blob bg-blob-delay-1 absolute -bottom-48 -right-48 w-[32rem] h-[32rem] rounded-full bg-primary/[0.05] dark:bg-primary/[0.03] blur-3xl" />
      <div class="bg-blob bg-blob-delay-2 absolute top-1/3 right-1/4 w-64 h-64 rounded-full bg-indigo-500/[0.04] dark:bg-indigo-500/[0.02] blur-3xl" />
    </div>

    <!-- 品牌标题区域 -->
    <header class="relative z-10 flex items-center gap-4 px-4 sm:px-6 lg:px-10 pt-6 sm:pt-8">
      <RouterLink to="/" class="flex items-center gap-3 no-underline shrink-0">
        <div class="relative">
          <div class="absolute inset-0 rounded-lg bg-primary/20 blur-md" />
          <div class="relative flex items-center justify-center w-9 h-9 rounded-lg bg-primary/10 border border-primary/20 backdrop-blur-sm">
            <MicVocal class="w-5 h-5 text-primary" />
          </div>
        </div>
        <div>
          <h1 class="text-base sm:text-lg font-bold tracking-tight text-foreground">MIMO TTS</h1>
          <p class="text-[10px] sm:text-xs text-muted-foreground/70 leading-none">智能语音合成</p>
        </div>
      </RouterLink>

      <!-- 导航 -->
      <nav class="ml-auto flex items-center gap-1">
        <RouterLink
          to="/"
          :class="['px-3 py-1.5 rounded-md text-xs font-medium transition-colors',
                   $route.path === '/' ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:text-foreground']">
          合成
        </RouterLink>
        <RouterLink
          to="/settings"
          :class="['px-3 py-1.5 rounded-md text-xs font-medium transition-colors',
                   $route.path === '/settings' ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:text-foreground']">
          设置
        </RouterLink>
      </nav>
    </header>

    <!-- 主内容 -->
    <main class="relative z-10 flex-1">
      <RouterView />
    </main>

    <!-- Footer -->
    <AppFooter />
  </div>
</template>

<script setup lang="ts">
import { RouterView, RouterLink } from 'vue-router'
import { MicVocal } from 'lucide-vue-next'
import AppFooter from '@/components/Footer.vue'
</script>
