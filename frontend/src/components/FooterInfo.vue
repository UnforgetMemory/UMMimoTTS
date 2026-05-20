<template>
  <footer 
    class="text-center py-6 sm:py-8 text-xs sm:text-sm text-muted-foreground"
    role="contentinfo"
    aria-label="页脚信息"
  >
    <div class="flex flex-col sm:flex-row items-center justify-center gap-2 sm:gap-4">
      <!-- License -->
      <span class="flex items-center gap-1">
        <svg class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 15h-2v-6h2v6zm4 0h-2v-6h2v6z"/>
        </svg>
        MIT License
      </span>
      
      <!-- Separator -->
      <span class="hidden sm:inline text-muted-foreground/50">·</span>
      
      <!-- GitHub Link -->
      <a 
        href="https://github.com/your-username/UMMimoTTS" 
        target="_blank" 
        rel="noopener noreferrer"
        class="hover:text-primary transition-colors flex items-center gap-1"
        aria-label="访问 GitHub 仓库"
      >
        <svg class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
        </svg>
        GitHub
      </a>
      
      <!-- Separator -->
      <span class="hidden sm:inline text-muted-foreground/50">·</span>
      
      <!-- Version -->
      <span>v2.0</span>
      
      <!-- Separator -->
      <span class="hidden sm:inline text-muted-foreground/50">·</span>
      
      <!-- Tech Stack -->
      <span class="flex items-center gap-2">
        <span class="px-2 py-0.5 rounded bg-orange-500/10 text-orange-600 dark:text-orange-400 text-xs">Rust</span>
        <span class="px-2 py-0.5 rounded bg-green-500/10 text-green-600 dark:text-green-400 text-xs">Vue</span>
        <span class="px-2 py-0.5 rounded bg-red-500/10 text-red-600 dark:text-red-400 text-xs">Actix</span>
      </span>
    </div>
    
    <!-- Copyright -->
    <p class="mt-2 text-xs text-muted-foreground/70">
      © 2026 MIMO TTS. All rights reserved.
    </p>
  </footer>
</template>

<script setup lang="ts">
// Footer Info Component - 底部开源信息
</script>
