<template>
  <Dialog :open="open" @update:open="onDialogClose">
    <DialogContent
      class="max-w-[95vw] w-full h-[90vh] flex flex-col p-0 sm:max-w-[90vw]"
      :show-close-button="false"
      prevent-overlay-close
    >
      <!-- Header -->
      <DialogHeader class="px-6 py-4 border-b shrink-0">
        <div class="flex items-center justify-between mb-3">
          <div>
            <DialogTitle>批量导入向导</DialogTitle>
            <DialogDescription>
              <span v-if="currentStep === 0">上传文件</span>
              <span v-else-if="currentStep === 1">设置分组和默认参数</span>
              <span v-else-if="currentStep === 2">编辑单个任务或使用分组默认值</span>
              <span v-else-if="currentStep === 3">确认并提交</span>
            </DialogDescription>
          </div>
          <Button variant="ghost" size="icon-sm" @click="onDialogClose">
            <XIcon class="w-4 h-4" />
          </Button>
        </div>
        <div class="flex items-center gap-2">
          <template v-for="(step, index) in steps" :key="index">
            <div class="flex items-center gap-2" :class="{ 'opacity-40': index > currentStep }">
              <div class="w-6 h-6 rounded-full flex items-center justify-center text-xs font-medium"
                :class="index <= currentStep ? 'bg-primary text-primary-foreground' : 'bg-muted text-muted-foreground'">
                {{ index + 1 }}
              </div>
              <span class="text-sm">{{ step.title }}</span>
            </div>
            <div v-if="index < steps.length - 1" class="w-8 h-px bg-border" />
          </template>
        </div>
      </DialogHeader>

      <!-- Body -->
      <div class="flex-1 min-h-0 overflow-y-auto px-6 py-4">
        <!-- Step 0: Upload -->
        <div v-if="currentStep === 0" class="flex flex-col gap-4">
          <div
            class="border-2 border-dashed rounded-lg p-12 flex flex-col items-center justify-center gap-4 cursor-pointer transition-colors"
            :class="isDragging ? 'border-primary bg-primary/5' : 'border-border hover:border-primary/50'"
            @click="triggerFileInput"
            @dragover.prevent="isDragging = true"
            @dragleave="isDragging = false"
            @drop.prevent="handleDrop"
          >
            <input ref="fileInputRef" type="file" multiple accept=".txt" webkitdirectory class="hidden" @change="handleFileSelect" />
            <UploadIcon class="w-12 h-12 text-muted-foreground" />
            <div class="text-center">
              <p class="text-sm font-medium">拖拽文件到此处，或点击选择文件</p>
              <p class="text-xs text-muted-foreground mt-1">支持 .txt 文件，可多选或选择文件夹批量导入</p>
            </div>
          </div>
          <div v-if="uploadState === 'uploading'" class="flex flex-col gap-2">
            <div class="flex items-center justify-between text-sm"><span>上传中...</span><span>{{ uploadProgress }}%</span></div>
            <Progress :value="uploadProgress" />
          </div>
          <div v-if="uploadState === 'error'" class="flex items-center gap-2 text-sm text-destructive bg-destructive/10 px-4 py-2 rounded-lg">
            <AlertCircleIcon class="w-4 h-4" />{{ uploadError }}
          </div>
          <div v-if="uploadState === 'success' && fileStats.length > 0" class="flex flex-col gap-3">
            <div class="flex items-center gap-2 text-sm text-green-600">
              <CheckCircleIcon class="w-4 h-4" />上传成功，共 {{ fileStats.length }} 个文件，{{ totalFileItems }} 条目
            </div>
            <div class="border rounded-lg overflow-hidden">
              <table class="w-full text-sm">
                <thead class="bg-muted/50">
                  <tr>
                    <th class="text-left px-3 py-2 font-medium"><button class="flex items-center gap-1 hover:text-foreground" @click="toggleSort('filename')">文件名<ArrowUpDownIcon class="w-3 h-3" /></button></th>
                    <th class="px-3 py-2 font-medium text-right"><button class="flex items-center justify-end gap-1 w-full hover:text-foreground" @click="toggleSort('item_count')">条目数<ArrowUpDownIcon class="w-3 h-3" /></button></th>
                    <th class="px-3 py-2 font-medium text-right"><button class="flex items-center justify-end gap-1 w-full hover:text-foreground" @click="toggleSort('char_count')">字符数<ArrowUpDownIcon class="w-3 h-3" /></button></th>
                    <th class="px-3 py-2 font-medium text-right"><button class="flex items-center justify-end gap-1 w-full hover:text-foreground" @click="toggleSort('token_count')">Tokens<ArrowUpDownIcon class="w-3 h-3" /></button></th>
                    <th class="w-10"></th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="stat in sortedFileStats" :key="stat.filename" class="border-t border-border/50 hover:bg-muted/30">
                    <td class="px-3 py-2 font-mono text-xs">{{ stat.filename }}</td>
                    <td class="px-3 py-2 text-right">{{ stat.item_count }}</td>
                    <td class="px-3 py-2 text-right">{{ stat.char_count.toLocaleString() }}</td>
                    <td class="px-3 py-2 text-right">{{ stat.token_count.toLocaleString() }}</td>
                    <td class="px-3 py-2 text-right">
                      <Button variant="ghost" size="icon-sm" @click="removeFile(stat.filename)">
                        <TrashIcon class="w-3 h-3" />
                      </Button>
                    </td>
                  </tr>
                </tbody>
                <tfoot class="bg-muted/30 border-t border-border/50">
                  <tr><td class="px-3 py-2 font-medium">合计</td><td class="px-3 py-2 text-right font-medium">{{ totalFileItems }}</td><td class="px-3 py-2 text-right font-medium">{{ totalChars.toLocaleString() }}</td><td class="px-3 py-2 text-right font-medium">{{ totalTokens.toLocaleString() }}</td><td></td></tr>
                </tfoot>
              </table>
            </div>
          </div>
        </div>

        <!-- Step 1: Group Defaults -->
        <div v-if="currentStep === 1" class="flex flex-col gap-6 flex-1 min-h-0 max-w-2xl mx-auto w-full">
          <div class="flex flex-col gap-4">
            <div class="flex flex-col gap-2">
              <Label for="groupName">批次名称（可选）</Label>
              <Input id="groupName" v-model="submitConfig.group_name" placeholder="留空则使用文件名" class="w-full" />
            </div>
            <div class="flex flex-col gap-2">
              <Label for="defaultVoice">默认音色 <span class="text-destructive">*</span></Label>
              <Select v-model="submitConfig.default_voice">
                <SelectTrigger id="defaultVoice"><SelectValue placeholder="选择音色" /></SelectTrigger>
                <SelectContent class="z-[9999]">
                  <SelectItem v-for="voice in voices" :key="voice.id" :value="voice.id">{{ voice.name }} ({{ voice.language }} / {{ voice.gender }})</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div class="flex flex-col gap-2">
              <Label for="defaultModel">默认模型</Label>
              <Select v-model="submitConfig.default_model">
                <SelectTrigger id="defaultModel"><SelectValue /></SelectTrigger>
                <SelectContent class="z-[9999]"><SelectItem value="mimo-v2.5-tts">mimo-v2.5-tts (预置音色)</SelectItem></SelectContent>
              </Select>
            </div>
            <div class="flex flex-col gap-2">
              <Label for="defaultContext">默认上下文/风格控制</Label>
              <Textarea id="defaultContext" v-model="submitConfig.default_context" placeholder="输入默认的上下文或风格控制文本..." rows="3" class="text-sm" />
              <p class="text-xs text-muted-foreground">如已在项目中逐个覆盖，此处可留空</p>
            </div>
          </div>
        </div>

        <!-- Step 2: Task List with paginated table -->
        <div v-if="currentStep === 2" class="flex flex-col gap-4 flex-1 min-h-0">
          <!-- Loading state -->
          <div v-if="previewState === 'loading'" class="flex items-center justify-center py-12 text-muted-foreground">
            <Loader2Icon class="w-5 h-5 animate-spin mr-2" />处理中...
          </div>

          <!-- Main content: table + pagination -->
          <template v-else-if="previewState === 'loaded'">
            <div class="flex items-center justify-between shrink-0">
              <span class="text-sm text-muted-foreground">共 {{ totalCount.toLocaleString() }} 条</span>
            </div>

            <!-- Table container -->
            <div class="border rounded-lg flex-1 min-h-0 flex flex-col">
              <!-- Scrollable table area -->
              <div class="flex-1 min-h-0 overflow-y-auto">
                <Table>
                  <TableHeader class="sticky top-0 bg-background z-10">
                    <TableRow>
                      <TableHead class="w-14">#</TableHead>
                      <TableHead>任务名</TableHead>
                      <TableHead class="w-44">风格</TableHead>
                      <TableHead class="w-36">音色</TableHead>
                      <TableHead class="w-24 text-right">Token</TableHead>
                      <TableHead class="w-12"></TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    <TableRow v-for="row in flattenedItems" :key="row.key">
                      <template v-if="row.type === 'data'">
                        <TableCell class="font-mono text-xs text-muted-foreground">{{ row.item.index + 1 }}</TableCell>
                        <TableCell>
                          <div class="text-sm truncate max-w-[200px]">{{ row.item.title || row.item.text_preview }}</div>
                        </TableCell>
                        <TableCell>
                          <span class="text-sm text-muted-foreground">{{ row.item.context || '默认' }}</span>
                        </TableCell>
                        <TableCell>
                          <span class="text-sm">{{ getVoiceName(row.item.voice || '') || '默认' }}</span>
                        </TableCell>
                        <TableCell class="text-right text-sm">{{ row.item.token_count?.toLocaleString() || '…' }}</TableCell>
                        <TableCell>
                          <Button variant="ghost" size="icon-sm" @click="toggleEditItem(row.item.index)">
                            <PencilIcon class="w-3 h-3" />
                          </Button>
                        </TableCell>
                      </template>
                      <TableCell v-else colspan="6" class="bg-muted/5 p-3">
                        <div class="flex flex-wrap items-end gap-3">
                          <div class="flex flex-col gap-1.5">
                            <Label class="text-xs">任务名</Label>
                            <Input v-model="editForm.title" class="h-7 text-xs w-44" placeholder="默认" />
                          </div>
                          <div class="flex flex-col gap-1.5">
                            <Label class="text-xs">风格</Label>
                            <Input v-model="editForm.context" class="h-7 text-xs w-44" placeholder="默认" />
                          </div>
                          <div class="flex flex-col gap-1.5">
                            <Label class="text-xs">音色</Label>
                            <Select v-model="editForm.voice">
                              <SelectTrigger class="h-7 text-xs w-36"><SelectValue placeholder="默认" /></SelectTrigger>
                              <SelectContent>
                                <SelectItem value="__default__">默认</SelectItem>
                                <SelectItem v-for="v in voices" :key="v.id" :value="v.id">{{ v.name }}</SelectItem>
                              </SelectContent>
                            </Select>
                          </div>
                          <div class="flex items-center gap-1.5">
                            <span v-if="editSaveStatus === 'saving'" class="text-[10px] text-muted-foreground flex items-center gap-1"><Loader2Icon class="w-3 h-3 animate-spin" />保存中...</span>
                            <span v-else-if="editSaveStatus === 'success'" class="text-[10px] text-green-500">✓ 已保存</span>
                            <span v-else-if="editSaveStatus === 'error'" class="text-[10px] text-destructive">保存失败</span>
                            <Button variant="ghost" size="sm" class="h-6 text-xs" @click="cancelEditItem">取消</Button>
                            <Button size="sm" class="h-6 text-xs" @click="handleSaveEdit(row.item)">保存</Button>
                          </div>
                        </div>
                      </TableCell>
                    </TableRow>
                  </TableBody>
                </Table>
              </div>
              <!-- Pagination - always visible at bottom -->
              <div class="bg-background border-t px-4 py-3 flex items-center justify-between shrink-0">
                <span class="text-sm text-muted-foreground">共 {{ totalCount.toLocaleString() }} 条</span>
                <div class="flex items-center gap-2">
                  <Button variant="outline" size="sm" :disabled="currentPage === 0" @click="goToPage(currentPage - 1)">上一页</Button>
                  <span class="text-sm text-muted-foreground">第 {{ currentPage + 1 }} / {{ totalPages }} 页</span>
                  <Button variant="outline" size="sm" :disabled="currentPage >= totalPages - 1" @click="goToPage(currentPage + 1)">下一页</Button>
                </div>
              </div>
            </div>
          </template>

          <!-- Error state -->
          <div v-if="previewState === 'error'" class="flex items-center justify-center py-12 gap-2 text-destructive">
            <AlertCircleIcon class="w-4 h-4" />{{ previewError }}
            <Button variant="outline" size="sm" @click="loadPage(currentPage)">重试</Button>
          </div>
          <!-- Empty state -->
          <div v-if="previewState === 'loaded' && totalCount === 0" class="flex flex-col items-center justify-center py-12 text-muted-foreground gap-2">
            <FileTextIcon class="w-12 h-12" /><p class="text-sm">没有可导入的任务</p>
          </div>
        </div>

        <!-- Step 3: Confirm & Submit -->
        <BatchImportPanel
          v-if="currentStep === 3"
          :total-count="totalCount"
          :default-voice="submitConfig.default_voice"
          :default-model="submitConfig.default_model"
          :group-name="submitConfig.group_name"
          :file-stats="fileStats"
          :total-chars="totalChars"
          :total-tokens="totalTokens"
          :voices="voices"
          :submit-error="submitError"
        />

        <!-- Step 4: Success -->
        <div v-if="currentStep === 4" class="flex flex-col items-center justify-center gap-4 py-12">
          <CheckCircleIcon class="w-16 h-16 text-green-500" />
          <h3 class="text-lg font-medium">导入成功</h3>
          <p class="text-sm text-muted-foreground">
            成功创建了 <strong>{{ submitResult.task_count }}</strong> 个任务，
            分组 ID: <code class="text-xs bg-muted px-1 py-0.5 rounded">{{ submitResult.group_id }}</code>
          </p>
          <Button @click="$emit('imported', submitResult.group_id)">查看任务</Button>
        </div>
      </div>

      <!-- Footer -->
      <DialogFooter class="px-6 py-4 border-t shrink-0">
        <div class="flex items-center justify-between w-full">
          <Button v-if="currentStep > 0 && currentStep < 4" variant="outline" @click="currentStep--">上一步</Button>
          <div v-else />
          <div class="flex items-center gap-2">
            <Button variant="outline" @click="onDialogClose">{{ currentStep === 4 ? '关闭' : '取消' }}</Button>
            <Button v-if="currentStep === 0" :disabled="uploadState !== 'success' || fileStats.length === 0" @click="currentStep = 1">下一步</Button>
            <Button v-if="currentStep === 1" :disabled="!submitConfig.default_voice" @click="handleStartPreview">下一步</Button>
            <Button v-if="currentStep === 2" :disabled="totalCount === 0" @click="currentStep = 3">下一步</Button>
            <Button v-if="currentStep === 3" :disabled="!submitConfig.default_voice || submitBusy" @click="handleSubmit">
              <Loader2Icon v-if="submitBusy" class="w-4 h-4 animate-spin mr-1" />创建任务
            </Button>
          </div>
        </div>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>

<script setup lang="ts">
import { ref, reactive, computed, watch, onMounted, onUnmounted } from 'vue'

import { apiV2, type Voice } from '@/api/client'
import { useTaskStore } from '@/stores/task'
import { useBatchStore } from '@/stores/batch'

/** Client-side parsed segment — extends preview data with full text */
interface ParsedSegment {
  index: number
  text: string
  text_preview: string
  title: string | null
  voice: string | null
  model: string | null
  context: string | null
  char_count: number
  token_count: number
  has_error: boolean
  error: string | null
  source_filename: string | null
}

/** Internal file stat tracked locally */
interface LocalFileStat {
  filename: string
  item_count: number
  char_count: number
  token_count: number
}
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import { Progress } from '@/components/ui/progress'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { UploadIcon, XIcon, FileTextIcon, AlertCircleIcon, CheckCircleIcon, TrashIcon, ArrowUpDownIcon, Loader2Icon, PencilIcon } from 'lucide-vue-next'
import BatchImportPanel from '@/components/BatchImportPanel.vue'

interface Props { open: boolean }
interface Emits { (e: 'update:open', v: boolean): void; (e: 'imported', g: string): void }
const props = defineProps<Props>()
const emit = defineEmits<Emits>()

const taskStore = useTaskStore()
const batchStore = useBatchStore()

const PER_PAGE = 50
const steps = [{ title: '上传文件' }, { title: '分组设置' }, { title: '自定义任务' }, { title: '确认提交' }]

// ── Upload state (client-side) ─────────────────────────────────────
const currentStep = ref(0)
const fileInputRef = ref<HTMLInputElement | null>(null)
const isDragging = ref(false)
type UploadState = 'idle' | 'uploading' | 'success' | 'error'
const uploadState = ref<UploadState>('idle')
const uploadProgress = ref(0)
const uploadError = ref('')

/** All parsed segments from file(s) — client-side only */
const parsedSegments = ref<ParsedSegment[]>([])

/** Local file stats derived from parsed segments */
const fileStats = ref<LocalFileStat[]>([])
const fileStatsSort = ref<{ key: string; dir: 'asc' | 'desc' }>({ key: 'filename', dir: 'asc' })
function removeFile(filename: string) {
  // Remove all segments belonging to this file
  const remaining = parsedSegments.value.filter(s => s.source_filename !== filename)
  parsedSegments.value = remaining.map((seg, idx) => ({ ...seg, index: idx }))
  fileStats.value = fileStats.value.filter(f => f.filename !== filename)
  if (currentPage.value >= totalPages.value) {
    currentPage.value = Math.max(0, totalPages.value - 1)
  }
  cancelEditItem()
}

function toggleSort(key: string) {
  if (fileStatsSort.value.key === key) { fileStatsSort.value.dir = fileStatsSort.value.dir === 'asc' ? 'desc' : 'asc' }
  else { fileStatsSort.value.key = key; fileStatsSort.value.dir = 'asc' }
}
const sortedFileStats = computed(() => {
  const sorted = [...fileStats.value]
  const key = fileStatsSort.value.key as keyof LocalFileStat
  const dir = fileStatsSort.value.dir === 'asc' ? 1 : -1
  sorted.sort((a, b) => { const av = a[key] as number | string; const bv = b[key] as number | string; if (typeof av === 'string' && typeof bv === 'string') return av.localeCompare(bv) * dir; return ((av as number) - (bv as number)) * dir })
  return sorted
})
const totalTokens = computed(() => fileStats.value.reduce((s, f) => s + f.token_count, 0))
const totalChars = computed(() => fileStats.value.reduce((s, f) => s + f.char_count, 0))
const totalFileItems = computed(() => fileStats.value.reduce((s, f) => s + f.item_count, 0))

// Preview state (local pagination)
type PreviewState = 'idle' | 'loading' | 'loaded' | 'error'
const previewState = ref<PreviewState>('idle')
const previewError = ref('')
const currentPage = ref(0)
const totalCount = computed(() => parsedSegments.value.length)
const totalPages = computed(() => Math.max(1, Math.ceil(totalCount.value / PER_PAGE)))
const currentPageItems = computed(() => {
  const start = currentPage.value * PER_PAGE
  return parsedSegments.value.slice(start, start + PER_PAGE)
})

type TableRowType = { key: string; type: 'data'; item: ParsedSegment } | { key: string; type: 'edit'; item: ParsedSegment }
const flattenedItems = computed<TableRowType[]>(() => {
  const rows: TableRowType[] = []
  for (const item of currentPageItems.value) {
    rows.push({ key: `d-${item.index}`, type: 'data', item })
    if (editingItemIndex.value === item.index) {
      rows.push({ key: `e-${item.index}`, type: 'edit', item })
    }
  }
  return rows
})

function toggleEditItem(itemIndex: number) {
  if (editingItemIndex.value === itemIndex) {
    cancelEditItem()
  } else {
    const item = parsedSegments.value.find(i => i.index === itemIndex)
    if (item) startEditItem(item)
  }
}

// Inline item edit state
const editingItemIndex = ref<number | null>(null)
const editSaveStatus = ref<'idle' | 'saving' | 'success' | 'error'>('idle')
const editForm = ref({ voice: '', model: '', title: '', context: '' })

function startEditItem(item: ParsedSegment) {
  editingItemIndex.value = item.index; editSaveStatus.value = 'idle'
  editForm.value = { voice: item.voice || '__default__', model: item.model || '', title: item.title || '', context: item.context || '' }
}

function cancelEditItem() { editingItemIndex.value = null; editSaveStatus.value = 'idle' }

function handleSaveEdit(item: ParsedSegment | null) {
  if (!item) return
  const idx = parsedSegments.value.findIndex(s => s.index === item.index)
  if (idx === -1) return
  editSaveStatus.value = 'saving'
  try {
    const updated = { ...parsedSegments.value[idx] }
    if (editForm.value.voice && editForm.value.voice !== '__default__') updated.voice = editForm.value.voice
    else updated.voice = null
    if (editForm.value.model) updated.model = editForm.value.model
    else updated.model = null
    if (editForm.value.title) updated.title = editForm.value.title
    else updated.title = null
    if (editForm.value.context) updated.context = editForm.value.context
    else updated.context = null
    const newSegments = [...parsedSegments.value]
    newSegments[idx] = updated
    parsedSegments.value = newSegments
    editSaveStatus.value = 'success'
    setTimeout(() => { if (editingItemIndex.value === item.index) cancelEditItem() }, 1500)
  } catch (err: unknown) {
    editSaveStatus.value = 'error'
    console.error('[BatchImport] Save edit failed:', err)
  }
}

// Voice helpers
function getVoiceName(voiceId: string): string { return voices.value.find(v => v.id === voiceId)?.name || voiceId }

// loadPage - pagination over client-side parsedSegments
function loadPage(page: number) {
  currentPage.value = page
  previewState.value = 'loaded'
  cancelEditItem()
}

function goToPage(page: number) {
  if (page < 0 || page >= totalPages.value) return
  editSaveStatus.value = 'idle'
  cancelEditItem()
  loadPage(page)
}

// Submit state
const submitConfig = reactive({ group_name: '', default_voice: '', default_model: 'mimo-v2.5-tts', default_context: '' })
const submitBusy = ref(false)
const submitError = ref('')
const submitResult = ref({ group_id: '', task_count: 0 })

// Step transitions
function handleStartPreview() { loadPage(0); currentStep.value = 2 }

async function handleSubmit() {
  if (parsedSegments.value.length === 0 || !submitConfig.default_voice) return
  submitBusy.value = true; submitError.value = ''
  try {
    // Step 1 – Create batch group
    const group = await batchStore.createGroup({
      name: submitConfig.group_name || undefined,
      voice: submitConfig.default_voice,
      model: submitConfig.default_model,
      context: submitConfig.default_context || undefined,
    })
    const groupId = group.id

    // Step 2 – Add all parsed segments as batch items (single request)
    const items = parsedSegments.value.map((seg, i) => ({
      seq: i + 1,
      filename: seg.source_filename || seg.title || `segment_${i + 1}.txt`,
      content: seg.text,
    }))
    await apiV2.addBatchItems(groupId, items)

    // Step 3 – Submit the batch (creates tasks + enqueues all at once)
    await apiV2.submitBatch(groupId)

    submitResult.value = { group_id: groupId, task_count: parsedSegments.value.length }
    currentStep.value = 4
    // Refresh task + group lists
    taskStore.loadTasks()
    batchStore.loadGroups()
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : '提交失败'
    submitError.value = msg
    console.error('[BatchImport] Submit failed:', msg)
  } finally {
    submitBusy.value = false
  }
}

function resetToUpload() {
  parsedSegments.value = []
  fileStats.value = []
  fileStatsSort.value = { key: 'filename', dir: 'asc' }
  currentPage.value = 0
  uploadState.value = 'idle'
  uploadProgress.value = 0
  uploadError.value = ''
  previewState.value = 'idle'
  previewError.value = ''
  submitError.value = ''
  currentStep.value = 0
}

function onDialogClose() { resetToUpload(); emit('update:open', false) }

// Voices
const FALLBACK_VOICES: Voice[] = [
  { id: '冰糖', name: '冰糖', language: '中文', gender: '女性', style: '活泼少女' },
  { id: '茉莉', name: '茉莉', language: '中文', gender: '女性', style: '知性女声' },
  { id: '苏打', name: '苏打', language: '中文', gender: '男性', style: '阳光少年' },
  { id: '白桦', name: '白桦', language: '中文', gender: '男性', style: '成熟男声' },
  { id: 'Mia', name: 'Mia', language: 'English', gender: 'Female', style: 'Lively girl' },
  { id: 'Chloe', name: 'Chloe', language: 'English', gender: 'Female', style: 'Sweet Dreamy' },
  { id: 'Milo', name: 'Milo', language: 'English', gender: 'Male', style: 'Sunny boy' },
  { id: 'Dean', name: 'Dean', language: 'English', gender: 'Male', style: 'Steady Gentle' },
]
const voices = ref<Voice[]>([])
async function loadVoices() {
  voices.value = FALLBACK_VOICES
}

// ── File reading helpers ───────────────────────────────────────────

/** Parse text content into segment blocks (split by blank lines) */
function parseTextIntoSegments(text: string, sourceFilename: string): ParsedSegment[] {
  const blocks = text
    .split(/\n\s*\n/)
    .map(b => b.trim())
    .filter(b => b.length > 0)

  return blocks.map((block, idx) => {
    const charCount = block.length
    const chineseChars = (block.match(/[\u4e00-\u9fff]/g) || []).length
    const englishWords = block.split(/\s+/).filter(w => w.length > 0).length
    const tokenCount = Math.ceil(chineseChars * 1.2 + englishWords * 0.8)
    const lines = block.split('\n')
    const firstLine = lines[0].trim()
    const title = firstLine.length < 60 ? firstLine : null
    const textPreview = block.substring(0, 120) + (block.length > 120 ? '...' : '')

    return {
      index: idx,
      text: block,
      text_preview: textPreview,
      title,
      voice: null,
      model: null,
      context: null,
      char_count: charCount,
      token_count: tokenCount,
      has_error: false,
      error: null,
      source_filename: sourceFilename,
    }
  })
}

function triggerFileInput() { fileInputRef.value?.click() }
function handleFileSelect(e: Event) {
  const input = e.target as HTMLInputElement
  const files = input.files
  if (!files || files.length === 0) return
  processFiles(Array.from(files))
  // Reset so same file can be re-selected
  input.value = ''
}

/** Recursively collect files from a FileSystemEntry (handles folders) */
async function collectFromEntry(entry: FileSystemEntry): Promise<File[]> {
  if (entry.isFile) {
    return new Promise((resolve) => { (entry as FileSystemFileEntry).file((f) => resolve([f])) })
  }
  if (entry.isDirectory) {
    const reader = (entry as FileSystemDirectoryEntry).createReader()
    const all: File[] = []
    const readBatch = async (): Promise<void> => {
      const entries = await new Promise<FileSystemEntry[]>((resolve) => { reader.readEntries(resolve) })
      if (entries.length === 0) return
      const results = await Promise.all(Array.from(entries).map((e) => collectFromEntry(e)))
      all.push(...results.flat())
      await readBatch()
    }
    await readBatch()
    return all
  }
  return []
}

async function handleDrop(e: DragEvent) {
  isDragging.value = false
  const items = e.dataTransfer?.items
  if (items && items.length > 0) {
    const entries: FileSystemEntry[] = []
    for (let i = 0; i < items.length; i++) {
      const entry = items[i].webkitGetAsEntry?.()
      if (entry) entries.push(entry)
    }
    if (entries.length > 0) {
      const fileArrays = await Promise.all(entries.map((entry) => collectFromEntry(entry)))
      const files = fileArrays.flat()
      if (files.length > 0) { processFiles(files); return }
    }
  }
  const files = e.dataTransfer?.files
  if (files && files.length > 0) { processFiles(Array.from(files)) }
}

/** Read .txt files client-side and parse into segments */
async function processFiles(files: File[]) {
  const txtFiles = files.filter(f => f.name.toLowerCase().endsWith('.txt'))
  if (txtFiles.length === 0) {
    uploadState.value = 'error'
    uploadError.value = '所选文件夹中没有 .txt 文件'
    return
  }

  uploadState.value = 'uploading'
  uploadProgress.value = 0
  uploadError.value = ''
  parsedSegments.value = []

  let allSegments: ParsedSegment[] = []
  const fileStatsArr: LocalFileStat[] = []

  for (let i = 0; i < txtFiles.length; i++) {
    const file = txtFiles[i]
    uploadProgress.value = Math.round(((i) / txtFiles.length) * 100)
    try {
      const text = await file.text()
      const segments = parseTextIntoSegments(text, file.name)
      allSegments = allSegments.concat(segments)
      fileStatsArr.push({
        filename: file.name,
        item_count: segments.length,
        char_count: segments.reduce((s, seg) => s + seg.char_count, 0),
        token_count: segments.reduce((s, seg) => s + seg.token_count, 0),
      })
    } catch (err) {
      console.error(`Failed to read file ${file.name}:`, err)
    }
  }

  // Re-index after combining
  parsedSegments.value = allSegments.map((seg, idx) => ({ ...seg, index: idx }))
  fileStats.value = fileStatsArr
  uploadProgress.value = 100
  previewState.value = 'loaded'
  uploadState.value = 'success'
}

onMounted(() => {
  loadVoices()
})

watch(() => props.open, (isOpen) => {
  if (isOpen) loadVoices()
})

onUnmounted(() => { /* cleanup */ })
</script>
