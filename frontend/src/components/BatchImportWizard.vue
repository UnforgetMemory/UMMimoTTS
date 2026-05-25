<template>
  <Dialog :open="open" @update:open="onDialogClose">
    <DialogContent
      class="max-w-[95vw] w-full h-[90vh] flex flex-col p-0 sm:max-w-[90vw]"
      :show-close-button="false"
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
                      <Button variant="ghost" size="icon-sm" :disabled="removingFile === stat.filename" @click="removeFile(stat.filename)">
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
          <div v-if="tokenExpired" class="flex items-center gap-2 text-sm text-destructive bg-destructive/10 px-4 py-2 rounded-lg">
            <AlertCircleIcon class="w-4 h-4 shrink-0" /><span>会话已过期，请重新上传文件</span>
            <Button variant="outline" size="sm" class="ml-auto" @click="resetToUpload">重新上传</Button>
          </div>

          <!-- Loading state -->
          <div v-if="previewState === 'loading'" class="flex items-center justify-center py-12 text-muted-foreground">
            <Loader2Icon class="w-5 h-5 animate-spin mr-2" />加载任务列表...
          </div>

          <!-- Main content: table + pagination -->
          <template v-else-if="previewState === 'loaded' && !tokenExpired">
            <div class="flex items-center justify-between shrink-0">
              <span class="text-sm text-muted-foreground">共 {{ totalCount.toLocaleString() }} 条</span>
            </div>

            <!-- Paginated table -->
            <div class="border rounded-lg overflow-hidden flex-1 min-h-0 flex flex-col">
              <Table>
                <TableHeader>
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
                  <template v-for="item in currentPageItems" :key="item.index">
                    <TableRow>
                      <TableCell class="font-mono text-xs text-muted-foreground">{{ item.index + 1 }}</TableCell>
                      <TableCell>
                        <div class="text-sm truncate max-w-[200px]">{{ item.title || item.text_preview }}</div>
                      </TableCell>
                      <TableCell>
                        <span class="text-sm text-muted-foreground">{{ item.context || '默认' }}</span>
                      </TableCell>
                      <TableCell>
                        <span class="text-sm">{{ getVoiceName(item.voice || '') || '默认' }}</span>
                      </TableCell>
                      <TableCell class="text-right text-sm">{{ item.token_count?.toLocaleString() || '…' }}</TableCell>
                      <TableCell>
                        <Button variant="ghost" size="icon-sm" @click="toggleEditItem(item.index)">
                          <PencilIcon class="w-3 h-3" />
                        </Button>
                      </TableCell>
                    </TableRow>
                    <!-- Inline edit form row -->
                    <TableRow v-if="editingItemIndex === item.index">
                      <TableCell colspan="6" class="bg-muted/5 p-3">
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
                                <SelectItem value="">默认</SelectItem>
                                <SelectItem v-for="v in voices" :key="v.id" :value="v.id">{{ v.name }}</SelectItem>
                              </SelectContent>
                            </Select>
                          </div>
                          <div class="flex items-center gap-1.5">
                            <span v-if="editSaveStatus === 'saving'" class="text-[10px] text-muted-foreground flex items-center gap-1"><Loader2Icon class="w-3 h-3 animate-spin" />保存中...</span>
                            <span v-else-if="editSaveStatus === 'success'" class="text-[10px] text-green-500">✓ 已保存</span>
                            <span v-else-if="editSaveStatus === 'error'" class="text-[10px] text-destructive">保存失败</span>
                            <Button variant="ghost" size="sm" class="h-6 text-xs" @click="cancelEditItem">取消</Button>
                            <Button size="sm" class="h-6 text-xs" @click="handleSaveEdit(item)">保存</Button>
                          </div>
                        </div>
                      </TableCell>
                    </TableRow>
                  </template>
                </TableBody>
              </Table>
            </div>

            <!-- Pagination -->
            <div class="flex items-center justify-between shrink-0">
              <span class="text-sm text-muted-foreground">共 {{ totalCount.toLocaleString() }} 条</span>
              <div class="flex items-center gap-2">
                <Button variant="outline" size="sm" :disabled="currentPage === 0" @click="goToPage(currentPage - 1)">上一页</Button>
                <span class="text-sm text-muted-foreground">第 {{ currentPage + 1 }} / {{ totalPages }} 页</span>
                <Button variant="outline" size="sm" :disabled="currentPage >= totalPages - 1" @click="goToPage(currentPage + 1)">下一页</Button>
              </div>
            </div>
          </template>

          <!-- Error state -->
          <div v-if="previewState === 'error' && !tokenExpired" class="flex items-center justify-center py-12 gap-2 text-destructive">
            <AlertCircleIcon class="w-4 h-4" />{{ previewError }}
            <Button variant="outline" size="sm" @click="loadPage(currentPage)">重试</Button>
          </div>
          <!-- Empty state -->
          <div v-if="previewState === 'loaded' && totalCount === 0 && !tokenExpired" class="flex flex-col items-center justify-center py-12 text-muted-foreground gap-2">
            <FileTextIcon class="w-12 h-12" /><p class="text-sm">没有可导入的任务</p>
          </div>
        </div>

        <!-- Step 3: Confirm & Submit -->
        <div v-if="currentStep === 3" class="flex flex-col gap-6 flex-1 min-h-0 max-w-2xl mx-auto w-full">
          <div class="flex flex-col gap-4">
            <h3 class="text-sm font-medium text-muted-foreground">确认提交</h3>
            <div class="border rounded-lg p-4 flex flex-col gap-3">
              <div class="flex items-center justify-between text-sm"><span class="text-muted-foreground">总条目数</span><span class="font-medium">{{ totalCount }} 个</span></div>
              <Separator />
              <div class="flex items-center justify-between text-sm"><span class="text-muted-foreground">默认音色</span><span class="font-medium">{{ submitConfig.default_voice ? getVoiceName(submitConfig.default_voice) : '未选择' }}</span></div>
              <Separator />
              <div class="flex items-center justify-between text-sm"><span class="text-muted-foreground">默认模型</span><span class="font-medium">{{ submitConfig.default_model }}</span></div>
              <Separator />
              <div class="flex items-center justify-between text-sm"><span class="text-muted-foreground">批次名称</span><span class="font-medium">{{ submitConfig.group_name || '自动生成' }}</span></div>
              <Separator />
              <div class="flex items-center justify-between text-sm"><span class="text-muted-foreground">文件数</span><span class="font-medium">{{ fileStats.length }} 个</span></div>
              <Separator />
              <div class="flex items-center justify-between text-sm"><span class="text-muted-foreground">总字符数</span><span class="font-medium">{{ totalChars.toLocaleString() }}</span></div>
              <Separator />
              <div class="flex items-center justify-between text-sm"><span class="text-muted-foreground">总 Tokens</span><span class="font-medium">{{ totalTokens.toLocaleString() }}</span></div>
            </div>
            <div v-if="submitError" class="flex items-center gap-2 text-sm text-destructive bg-destructive/10 px-4 py-2 rounded-lg">
              <AlertCircleIcon class="w-4 h-4" />{{ submitError }}
            </div>
          </div>
        </div>

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
            <Button v-if="currentStep === 2" :disabled="tokenExpired || totalCount === 0" @click="currentStep = 3">下一步</Button>
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

import { api, type ParsedItem, type FileStat, type Voice } from '@/api/client'
import { useTaskStore } from '@/stores/task'
import { useBatchStore } from '@/stores/batch'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import { Separator } from '@/components/ui/separator'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { UploadIcon, XIcon, ChevronDownIcon, FileTextIcon, AlertCircleIcon, CheckCircleIcon, TrashIcon, ArrowUpDownIcon, Loader2Icon, PencilIcon } from 'lucide-vue-next'

interface Props { open: boolean }
interface Emits { (e: 'update:open', v: boolean): void; (e: 'imported', g: string): void }
const props = defineProps<Props>()
const emit = defineEmits<Emits>()

const taskStore = useTaskStore()
const batchStore = useBatchStore()

const EXTEND_INTERVAL_MS = 240_000
const PER_PAGE = 50
const steps = [{ title: '上传文件' }, { title: '分组设置' }, { title: '自定义任务' }, { title: '确认提交' }]

// Upload state
const currentStep = ref(0)
const fileInputRef = ref<HTMLInputElement | null>(null)
const isDragging = ref(false)
type UploadState = 'idle' | 'uploading' | 'success' | 'error'
const uploadState = ref<UploadState>('idle')
const uploadProgress = ref(0)
const uploadError = ref('')
const fileStats = ref<FileStat[]>([])
const fileStatsSort = ref<{ key: string; dir: 'asc' | 'desc' }>({ key: 'filename', dir: 'asc' })
const removingFile = ref('')

function toggleSort(key: string) {
  if (fileStatsSort.value.key === key) { fileStatsSort.value.dir = fileStatsSort.value.dir === 'asc' ? 'desc' : 'asc' }
  else { fileStatsSort.value.key = key; fileStatsSort.value.dir = 'asc' }
}
const sortedFileStats = computed(() => {
  const sorted = [...fileStats.value]
  const key = fileStatsSort.value.key as keyof FileStat
  const dir = fileStatsSort.value.dir === 'asc' ? 1 : -1
  sorted.sort((a, b) => { const av = a[key]; const bv = b[key]; if (typeof av === 'string' && typeof bv === 'string') return av.localeCompare(bv) * dir; return ((av as number) - (bv as number)) * dir })
  return sorted
})
const totalTokens = computed(() => fileStats.value.reduce((s, f) => s + f.token_count, 0))
const totalChars = computed(() => fileStats.value.reduce((s, f) => s + f.char_count, 0))
const totalFileItems = computed(() => fileStats.value.reduce((s, f) => s + f.item_count, 0))

async function removeFile(filename: string) {
  if (!importToken.value) return
  removingFile.value = filename
  try {
    await api.removeBatchImportFile(importToken.value, filename)
    fileStats.value = fileStats.value.filter(f => f.filename !== filename)
    totalCount.value = fileStats.value.reduce((s, f) => s + f.item_count, 0)
    cancelEditItem()
    currentPage.value = 0
    await loadPage(0)
  }
  catch (err) { console.error('Failed to remove file:', err) }
  finally { removingFile.value = '' }
}

// Token / session state
const importToken = ref('')
const totalCount = ref(0)
const uploadedFileList = ref<string[]>([])

// Preview state (paginated)
type PreviewState = 'idle' | 'loading' | 'loaded' | 'error'
const previewState = ref<PreviewState>('idle')
const previewError = ref('')
const currentPageItems = ref<ParsedItem[]>([])
const currentPage = ref(0)
const tokenExpired = ref(false)
const totalPages = computed(() => Math.max(1, Math.ceil(totalCount.value / PER_PAGE)))

function toggleEditItem(itemIndex: number) {
  if (editingItemIndex.value === itemIndex) {
    cancelEditItem()
  } else {
    const item = currentPageItems.value.find(i => i.index === itemIndex)
    if (item) startEditItem(item)
  }
}

// Inline item edit state
const editingItemIndex = ref<number | null>(null)
const editSaveStatus = ref<'idle' | 'saving' | 'success' | 'error'>('idle')
const editForm = ref({ voice: '', model: '', title: '', context: '' })

function hasItemOverride(item: ParsedItem): boolean { return !!(item.voice || item.model || item.title) }

function startEditItem(item: ParsedItem) {
  editingItemIndex.value = item.index; editSaveStatus.value = 'idle'
  editForm.value = { voice: item.voice || '', model: item.model || '', title: item.title || '', context: item.context || '' }
}

function cancelEditItem() { editingItemIndex.value = null; editSaveStatus.value = 'idle' }

function getEditingItem(): ParsedItem | null {
  if (editingItemIndex.value === null) return null
  return currentPageItems.value.find(i => i.index === editingItemIndex.value) || null
}

async function handleSaveEdit(item: ParsedItem | null) {
  if (!importToken.value || !item) return
  editSaveStatus.value = 'saving'
  try {
    const ov: Record<string, string> = {}
    if (editForm.value.voice) ov.voice = editForm.value.voice
    if (editForm.value.model) ov.model = editForm.value.model
    if (editForm.value.title) ov.custom_title = editForm.value.title
    if (editForm.value.context) ov.context = editForm.value.context
    const updated = await api.updateBatchImportItem(importToken.value, item.index, ov as any)
    const idx = currentPageItems.value.findIndex(i => i.index === item.index)
    if (idx !== -1) currentPageItems.value[idx] = updated
    editSaveStatus.value = 'success'
    setTimeout(() => { if (editingItemIndex.value === item.index) cancelEditItem() }, 1500)
  } catch (err: unknown) { editSaveStatus.value = 'error'; console.error('[BatchImport] Save edit failed:', err) }
}

// Voice helpers
function getVoiceName(voiceId: string): string { return voices.value.find(v => v.id === voiceId)?.name || voiceId }

// loadPage - fetches a specific page of preview items
async function loadPage(page: number) {
  if (!importToken.value) return
  previewState.value = 'loading'
  try {
    const result = await api.getBatchImportPreview(importToken.value, page, PER_PAGE)
    currentPageItems.value = result.items
    totalCount.value = result.total
    currentPage.value = page
    previewState.value = 'loaded'
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : '加载预览失败'
    if (msg.includes('410') || msg.includes('404') || msg.includes('expired') || msg.includes('not found')) { tokenExpired.value = true; previewState.value = 'idle' }
    else { previewState.value = 'error'; previewError.value = msg }
  }
}

function goToPage(page: number) {
  if (page < 0 || page >= totalPages.value) return
  editSaveStatus.value = 'idle'
  cancelEditItem()
  loadPage(page)
}

// Session extend timer
let extendTimer: ReturnType<typeof setInterval> | null = null
function startExtendTimer() {
  stopExtendTimer()
  extendTimer = setInterval(async () => {
    if (!importToken.value || tokenExpired.value) return
    try { await api.extendBatchImportSession(importToken.value) } catch { tokenExpired.value = true; stopExtendTimer() }
  }, EXTEND_INTERVAL_MS)
}
function stopExtendTimer() { if (extendTimer !== null) { clearInterval(extendTimer); extendTimer = null } }

// Submit state
const submitConfig = reactive({ group_name: '', default_voice: '', default_model: 'mimo-v2.5-tts', default_context: '' })
const submitBusy = ref(false)
const submitError = ref('')
const submitResult = ref({ group_id: '', task_count: 0 })

// Step transitions
function startPreview() { loadPage(0); startExtendTimer() }
function handleStartPreview() { startPreview(); currentStep.value = 2 }

async function handleSubmit() {
  if (!importToken.value || !submitConfig.default_voice) return
  submitBusy.value = true; submitError.value = ''
  try {
    const result = await api.submitBatchImport(importToken.value, {
      default_voice: submitConfig.default_voice, default_model: submitConfig.default_model,
      default_context: submitConfig.default_context, group_name: submitConfig.group_name,
    })
    submitResult.value = result; currentStep.value = 4; stopExtendTimer()
    // Refresh BOTH task list AND batch group list so new tasks appear immediately
    taskStore.loadTasks(); batchStore.loadGroups()
  } catch (err: unknown) { const msg = err instanceof Error ? err.message : '提交失败'; submitError.value = msg; console.error('[BatchImport] Submit failed:', msg) }
  finally { submitBusy.value = false }
}

function resetToUpload() {
  stopExtendTimer(); importToken.value = ''; totalCount.value = 0
  currentPageItems.value = []; currentPage.value = 0
  uploadState.value = 'idle'; uploadProgress.value = 0
  uploadError.value = ''; uploadedFileList.value = []; fileStats.value = []
  fileStatsSort.value = { key: 'filename', dir: 'asc' }; removingFile.value = ''
  previewState.value = 'idle'; previewError.value = ''; tokenExpired.value = false
  submitError.value = ''; currentStep.value = 0
}

function onDialogClose() { stopExtendTimer(); emit('update:open', false) }

// Voices
const voices = ref<Voice[]>([])
async function loadVoices() { try { voices.value = await api.getVoices() } catch { /* non-critical */ } }

// File handling
function triggerFileInput() { fileInputRef.value?.click() }
function handleFileSelect(e: Event) { const input = e.target as HTMLInputElement; const files = input.files; if (!files || files.length === 0) return; processFiles(Array.from(files)) }

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
  // Phase 1: Use webkitGetAsEntry() for recursive folder support
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
  // Phase 2: Fallback to flat file list
  const files = e.dataTransfer?.files
  if (files && files.length > 0) { processFiles(Array.from(files)) }
}

async function processFiles(files: File[]) {
  const txtFiles = files.filter(f => f.name.toLowerCase().endsWith('.txt'))
  if (txtFiles.length === 0) { uploadState.value = 'error'; uploadError.value = '所选文件夹中没有 .txt 文件'; return }
  if (txtFiles.length === 1) { uploadFile(txtFiles[0]); return }
  let combined = ''
  for (const f of txtFiles) { const text = await f.text(); combined += `# ${f.name}\n${text}\n` }
  const virtualFile = new File([combined], 'batch_import.txt', { type: 'text/plain' })
  uploadFile(virtualFile)
}

async function uploadFile(file: File) {
  uploadState.value = 'uploading'; uploadProgress.value = 0; uploadError.value = ''
  tokenExpired.value = false; uploadedFileList.value = [file.name]
  try {
    const result = await api.uploadBatchFile(file, (pct) => { uploadProgress.value = pct })
    importToken.value = result.token
    totalCount.value = result.stats?.valid_items ?? result.stats?.total_items ?? 0
    fileStats.value = result.file_stats ?? result.stats?.file_stats ?? []
    uploadState.value = 'success'
  } catch (err: unknown) { uploadState.value = 'error'; uploadError.value = err instanceof Error ? err.message : '上传失败，请重试' }
}

onMounted(() => {
  loadVoices()
})

watch(() => props.open, (isOpen) => {
  if (isOpen) loadVoices()
})

onUnmounted(() => { stopExtendTimer() })
</script>
