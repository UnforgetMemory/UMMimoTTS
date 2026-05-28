<template>
  <div class="flex flex-col gap-6 flex-1 min-h-0 max-w-2xl mx-auto w-full">
    <div class="flex flex-col gap-4">
      <h3 class="text-sm font-medium text-muted-foreground">确认提交</h3>
      <div class="border rounded-lg p-4 flex flex-col gap-3">
        <div class="flex items-center justify-between text-sm">
          <span class="text-muted-foreground">总条目数</span>
          <span class="font-medium">{{ totalCount }} 个</span>
        </div>
        <Separator />
        <div class="flex items-center justify-between text-sm">
          <span class="text-muted-foreground">默认音色</span>
          <span class="font-medium">{{ defaultVoice ? getVoiceName(defaultVoice) : '未选择' }}</span>
        </div>
        <Separator />
        <div class="flex items-center justify-between text-sm">
          <span class="text-muted-foreground">默认模型</span>
          <span class="font-medium">{{ defaultModel }}</span>
        </div>
        <Separator />
        <div class="flex items-center justify-between text-sm">
          <span class="text-muted-foreground">批次名称</span>
          <span class="font-medium">{{ groupName || '自动生成' }}</span>
        </div>
        <Separator />
        <div class="flex items-center justify-between text-sm">
          <span class="text-muted-foreground">文件数</span>
          <span class="font-medium">{{ fileStats.length }} 个</span>
        </div>
        <Separator />
        <div class="flex items-center justify-between text-sm">
          <span class="text-muted-foreground">总字符数</span>
          <span class="font-medium">{{ totalChars.toLocaleString() }}</span>
        </div>
        <Separator />
        <div class="flex items-center justify-between text-sm">
          <span class="text-muted-foreground">总 Tokens</span>
          <span class="font-medium">{{ totalTokens.toLocaleString() }}</span>
        </div>
      </div>

      <!-- File Grouping Preview -->
      <div class="border rounded-lg p-4 flex flex-col gap-3">
        <h4 class="text-xs font-medium text-muted-foreground uppercase tracking-wider">文件任务分组</h4>
        <div class="flex flex-col divide-y">
          <div class="flex items-center gap-4 py-2 text-xs text-muted-foreground font-medium">
            <span class="flex-1 min-w-0">文件名</span>
            <span class="w-16 text-right">条目数</span>
            <span class="w-20 text-right">字符数</span>
            <span class="w-14 text-right">任务数</span>
          </div>
          <div v-for="f in fileStats" :key="f.filename" class="flex items-center gap-4 py-2 text-sm">
            <span class="flex-1 min-w-0 truncate" :title="f.filename">{{ f.filename }}</span>
            <span class="w-16 text-right text-muted-foreground">{{ f.item_count }}</span>
            <span class="w-20 text-right text-muted-foreground">{{ f.char_count.toLocaleString() }}</span>
            <span class="w-14 text-right font-medium">1</span>
          </div>
        </div>
        <Separator />
        <div class="flex items-center justify-between text-xs text-muted-foreground">
          <span>共 <strong>{{ fileStats.length }}</strong> 个文件 → <strong>{{ fileStats.length }}</strong> 个合成任务</span>
          <span>总计 {{ totalChars.toLocaleString() }} 字符</span>
        </div>
      </div>

      <div v-if="submitError" class="flex items-center gap-2 text-sm text-destructive bg-destructive/10 px-4 py-2 rounded-lg">
        <AlertCircleIcon class="w-4 h-4" />{{ submitError }}
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { Voice } from '@/api/client'
import { Separator } from '@/components/ui/separator'
import { AlertCircleIcon } from 'lucide-vue-next'

interface LocalFileStat {
  filename: string
  item_count: number
  char_count: number
  token_count: number
}

const props = defineProps<{
  totalCount: number
  defaultVoice: string
  defaultModel: string
  groupName: string
  fileStats: LocalFileStat[]
  totalChars: number
  totalTokens: number
  voices: Voice[]
  submitError: string
}>()

function getVoiceName(voiceId: string): string {
  return props.voices.find(v => v.id === voiceId)?.name || voiceId
}
</script>
