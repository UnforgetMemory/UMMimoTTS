<template>
  <div
    class="fixed inset-0 flex items-center justify-center pointer-events-none select-none overflow-hidden"
    style="z-index: 1;"
    aria-hidden="true"
  >
    <h1
      class="text-[24vw] sm:text-[22vw] md:text-[20vw] lg:text-[18vw] xl:text-[16vw] 2xl:text-[14vw] 3xl:text-[12vw] 4xl:text-[10vw] font-black tracking-tighter whitespace-nowrap"
      :style="{
        lineHeight: '0.85',
        background: 'linear-gradient(135deg, oklch(0.5 0.25 260 / 0.12) 0%, oklch(0.6 0.2 280 / 0.08) 50%, oklch(0.5 0.22 300 / 0.10) 100%)',
        WebkitBackgroundClip: 'text',
        WebkitTextFillColor: 'transparent',
        backgroundClip: 'text',
        filter: 'blur(0.5px)',
      }"
    >
      UM-MimoTTS
    </h1>
  </div>
</template>

<script setup lang="ts">
// Brand Hero — 渐变色超大字号背景水印
</script>
