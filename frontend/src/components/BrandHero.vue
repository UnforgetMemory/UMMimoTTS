<template>
  <div class="brand-hero text-center py-8 sm:py-12 md:py-16">
    <h1 
      class="brand-title text-4xl sm:text-5xl md:text-6xl lg:text-7xl font-bold tracking-tight mb-3 sm:mb-4"
      aria-label="MIMO TTS 语音合成服务"
    >
      MIMO TTS
    </h1>
    <p class="brand-subtitle text-base sm:text-lg md:text-xl text-muted-foreground max-w-2xl mx-auto px-4">
      基于 MIMO v2.5 模型的高品质语音合成服务
    </p>
    <span 
      class="version-badge inline-block mt-4 sm:mt-6 px-3 py-1 text-xs sm:text-sm font-medium rounded-full bg-primary/10 text-primary border border-primary/20"
      aria-label="版本号 2.0"
    >
      v2.0
    </span>
  </div>
</template>

<script setup lang="ts">
// Brand Hero Component - 艺术化品牌展示
</script>

<style scoped>
.brand-title {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 50%, #f093fb 100%);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
  animation: pulseGlow 3s ease-in-out infinite;
  will-change: filter;
}

/* 暗色模式适配 */
@media (prefers-color-scheme: dark) {
  .brand-title {
    background: linear-gradient(135deg, #a78bfa 0%, #c084fc 50%, #e879f9 100%);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }
}

@keyframes pulseGlow {
  0%, 100% { 
    filter: drop-shadow(0 0 8px rgba(102, 126, 234, 0.4)); 
  }
  50% { 
    filter: drop-shadow(0 0 20px rgba(118, 75, 162, 0.8)); 
  }
}

/* 暗色模式下调整发光强度 */
@media (prefers-color-scheme: dark) {
  @keyframes pulseGlow {
    0%, 100% { 
      filter: drop-shadow(0 0 12px rgba(167, 139, 250, 0.5)); 
    }
    50% { 
      filter: drop-shadow(0 0 28px rgba(192, 132, 252, 0.9)); 
    }
  }
}

.version-badge {
  transition: all 0.3s ease;
}

.version-badge:hover {
  transform: scale(1.05);
  box-shadow: 0 4px 12px rgba(102, 126, 234, 0.3);
}
</style>
