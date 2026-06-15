<template>
  <footer
    class="relative z-10 border-t border-border/60 backdrop-blur-md bg-white/60 dark:bg-gray-950/60 py-5 text-xs text-muted-foreground"
    role="contentinfo"
    aria-label="页脚信息"
  >
    <div class="flex flex-col sm:flex-row items-center justify-center gap-1.5 sm:gap-3 flex-wrap px-3 sm:px-4 lg:px-6">
      <!-- License -->
      <span class="flex items-center gap-1">
        <ScaleIcon class="w-3 h-3" />
        MIT License
      </span>

      <span class="hidden sm:inline text-muted-foreground/40">·</span>

      <!-- GitHub -->
      <a
        href="https://github.com/UnforgetMemory/UMMimoTTS"
        target="_blank"
        rel="noopener noreferrer"
        class="hover:text-primary transition-colors flex items-center gap-1"
        aria-label="GitHub 仓库"
      >
        <GithubIcon class="w-3 h-3" />
        GitHub
      </a>

      <span class="hidden sm:inline text-muted-foreground/40">·</span>

      <!-- Versions -->
      <span class="flex items-center gap-1.5 flex-wrap justify-center">
        <span v-if="frontendVersion" class="px-1.5 py-0.5 rounded bg-blue-500/10 text-blue-600 dark:text-blue-400 text-[10px] font-medium">
          FE v{{ frontendVersion }}
        </span>
        <span v-if="backendVersion" class="px-1.5 py-0.5 rounded bg-orange-500/10 text-orange-600 dark:text-orange-400 text-[10px] font-medium">
          BE v{{ backendVersion }}
        </span>
      </span>

      <span class="hidden sm:inline text-muted-foreground/40">·</span>

      <!-- Tech Stack -->
      <span class="flex items-center gap-1.5">
        <span class="px-1.5 py-0.5 rounded bg-orange-500/10 text-orange-600 dark:text-orange-400 text-[10px] font-medium">Rust</span>
        <span class="px-1.5 py-0.5 rounded bg-green-500/10 text-green-600 dark:text-green-400 text-[10px] font-medium">Vue</span>
        <span class="px-1.5 py-0.5 rounded bg-red-500/10 text-red-600 dark:text-red-400 text-[10px] font-medium">Actix</span>
      </span>
    </div>

    <!-- Copyright -->
    <p class="mt-2 text-center text-[10px] text-muted-foreground/60">
      © 2026 UM-MIMO-TTS. All rights reserved.
    </p>
  </footer>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { Scale as ScaleIcon, Github as GithubIcon } from 'lucide-vue-next'
import apiClient from '@/api/client'

const frontendVersion = ref('')
const backendVersion = ref('')

onMounted(async () => {
  frontendVersion.value = __APP_VERSION__ || '3.0.0'
  try {
    const { data } = await apiClient.get('/api/version')
    backendVersion.value = data?.version || ''
  } catch { /* ignore */ }
})
</script>
