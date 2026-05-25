# Batch Import Wizard Refactor — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor Batch Import Wizard so ALL computation happens on the Rust backend. Frontend only renders backend-provided data. 4-step wizard: Upload → Group Defaults → Custom Tasks → Submit.

**Core Principle:** Backend Rust computes `token_count`, `char_count`, per-file stats, and provides them via API. Frontend does zero computation except rendering.

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `backend/src/models/batch_import.rs` | MODIFY | Add `source_filename`, `token_count` to ParsedItem/ParsedItemSummary; add `FileStat` struct |
| `backend/src/services/batch_import.rs` | MODIFY | Parse `# filename` headers during upload; compute token_count; add `file_stats()` method |
| `backend/src/routes/batch_import.rs` | MODIFY | Add `file_stats` to upload response; add `token_count` to preview response |
| `frontend/src/api/client.ts` | MODIFY | Add `token_count`, `source_filename`, `FileStat` types |
| `frontend/src/components/BatchImportWizard.vue` | MODIFY | Restructure steps, add file stats table, add group defaults step, refactor custom tasks step |
| `frontend/src/components/__tests__/BatchImportWizard.import.test.ts` | MODIFY | Update tests for new step structure |

**No new files. No new dependencies.**

---

## Architecture Decisions

### Backend Changes (REQUIRED)

#### 1. Parse `# filename` headers in upload handler

Current upload handler treats `# filename.txt` lines as regular text items. Change: recognize lines starting with `# ` followed by a filename as **file delimiters**, set `source_filename` on all subsequent items until the next `# ` line.

```rust
// In ParsedItem, add:
pub source_filename: Option<String>,
pub token_count: usize,  // computed: ceil(char_count / 1.5) for CJK, char_count for ASCII

// In ParsedItemSummary, add:
pub source_filename: Option<String>,
pub token_count: usize,
```

#### 2. Token counting heuristic

For Chinese TTS text, `token_count ≈ ceil(char_count / 1.5)` is reasonable. For mixed content, use `char_count` as upper bound. Implementation:

```rust
fn compute_token_count(text: &str) -> usize {
    let char_count = text.chars().count();
    let cjk_count = text.chars().filter(|c| c > '\u{2E80}').count();
    if cjk_count > char_count / 2 {
        // Predominantly CJK — tokens ~ chars * 0.67
        (char_count as f64 / 1.5).ceil() as usize
    } else {
        // Predominantly ASCII — tokens ~ chars / 4 (byte-pair-like)
        (char_count as f64 / 4.0).ceil() as usize
    }
}
```

#### 3. `FileStat` struct for per-file aggregates

```rust
#[derive(Debug, Clone, Serialize)]
pub struct FileStat {
    pub filename: String,
    pub item_count: usize,
    pub char_count: usize,
    pub token_count: usize,
}
```

#### 4. Add `file_stats` to upload response and `ImportStats`

```rust
pub struct ImportStats {
    pub total_items: usize,
    pub valid_items: usize,
    pub error_items: usize,
    pub total_chars: usize,
    pub total_token_count: usize,  // NEW
    pub file_stats: Vec<FileStat>,   // NEW
    pub created_at: String,
    pub expires_at: String,
}
```

#### 5. Upload handler: parse `# filename` headers and track file boundaries

During `upload_file`, iterate lines and detect `# ` prefix as file header. Group items by source filename.

#### 6. Preview handler: include `source_filename` and `token_count`

`ParsedItemSummary` already has `char_count`. Add `source_filename` and `token_count`.

### Step Restructure

| Current | New | Content |
|---------|-----|---------|
| Step 0: 上传文件 | Step 0: 上传文件 | Drag-drop zone + **file stats TABLE** from backend (`file_stats` in upload response) |
| Step 1: 预览编辑 | Step 1: 分组默认配置 | Group defaults form (group_name, default_voice*, default_model, default_context) |
| Step 2: 提交设置 | Step 2: 自定义任务 | Virtual-scroll item list with inline overrides — data fetched via pagination |
| Step 3: 完成 | Step 3: 提交任务 | Confirm summary → submit button → done message |

### Data Flow

```
Upload file/folder → POST /batch/upload
  ↳ Response: { token, stats: { file_stats: [{filename, item_count, char_count, token_count}] } }
  ↳ Frontend Step 0: renders file_stats table (already available from upload response!)

Step 1: Group defaults form (no backend call, pure form state)

Step 2: GET /batch/preview?token=xxx&page=0&per_page=50
  ↳ Response: { items: [{index, text_preview, source_filename, char_count, token_count, voice, model, title, has_error}], total, page, per_page, total_pages }
  ↳ Frontend: virtual-scroll list with load-more on scroll
  ↳ PUT /batch/items/{index} to save per-item overrides

Step 3: POST /batch/submit with group defaults + token
```

---

## Implementation Tasks

### Phase A: Backend — Models & Parsing

#### Task A1: Add `source_filename` and `token_count` to `ParsedItem`

- [ ] A1a: Write failing test for `ParsedItem::to_summary()` including `source_filename` and `token_count`
- [ ] A1b: Add `source_filename: Option<String>` and `token_count: usize` to `ParsedItem` in `backend/src/models/batch_import.rs`
- [ ] A1c: Add `source_filename: Option<String>` and `token_count: usize` to `ParsedItemSummary`
- [ ] A1d: Update `ParsedItem::to_summary()` to include new fields
- [ ] A1e: Run `cargo test` — verify test passes

**File:** `backend/src/models/batch_import.rs`

```rust
pub struct ParsedItem {
    pub index: usize,
    pub text: String,
    pub voice: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    pub context: Option<String>,
    pub speed: Option<f32>,
    pub source_filename: Option<String>,  // NEW: which file this item came from
    pub token_count: usize,                  // NEW: backend-computed token estimate
    pub error: Option<String>,
}

pub struct ParsedItemSummary {
    pub index: usize,
    pub text_preview: String,
    pub voice: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    pub source_filename: Option<String>,  // NEW
    pub char_count: usize,
    pub token_count: usize,                // NEW
    pub has_error: bool,
    pub error: Option<String>,
}
```

#### Task A2: Add `FileStat` struct and update `ImportStats`

- [ ] A2a: Add `FileStat` struct to `backend/src/models/batch_import.rs`
- [ ] A2b: Add `total_token_count: usize` and `file_stats: Vec<FileStat>` to `ImportStats`
- [ ] A2c: Update `PendingImport::stats()` to compute `file_stats` and `total_token_count`
- [ ] A2d: Run `cargo test`

**File:** `backend/src/models/batch_import.rs`

```rust
#[derive(Debug, Clone, Serialize)]
pub struct FileStat {
    pub filename: String,
    pub item_count: usize,
    pub char_count: usize,
    pub token_count: usize,
}

pub struct ImportStats {
    pub total_items: usize,
    pub valid_items: usize,
    pub error_items: usize,
    pub total_chars: usize,
    pub total_token_count: usize,   // NEW
    pub file_stats: Vec<FileStat>,    // NEW
    pub created_at: String,
    pub expires_at: String,
}
```

#### Task A3: Implement `# filename` header parsing in upload handler

- [ ] A3a: Write failing test for upload handler that parses `# filename.txt` headers and assigns `source_filename`
- [ ] A3b: Implement `# filename` header detection in `upload_file()` route handler in `backend/src/routes/batch_import.rs`
- [ ] A3c: Add `compute_token_count()` function to `batch_import.rs` model or route
- [ ] A3d: Set `source_filename` on items parsed after each `# filename` header
- [ ] A3e: Set `token_count` via `compute_token_count()` on each item
- [ ] A3f: Run `cargo test`

**Logic in upload handler:**

```rust
let mut current_source: Option<String> = None;
for (i, line) in lines.into_iter().enumerate() {
    let trimmed = line.trim();
    if trimmed.starts_with("# ") && trimmed.ends_with(".txt") {
        // File delimiter line — update current source, skip as content
        current_source = Some(trimmed[2..].to_string());
        continue; // don't create a ParsedItem for this line
    }
    // ... parse as ParsedItem, set source_filename: current_source.clone(), token_count: compute_token_count(text)
}
```

#### Task A4: Update `BatchImportManager::mark_submitted` and submit handler

- [ ] A4a: Verify submit handler still works with new `ParsedItem` fields (it reads items from memory, so it should be compatible)
- [ ] A4b: Verify `TtsTask.char_count` uses `text.len()` — consider using `text.chars().count()` instead for Unicode accuracy
- [ ] A4c: Run `cargo test`

---

### Phase B: Backend — API Response Updates

#### Task B1: Add `file_stats` to upload response

- [ ] B1a: Verify `stats()` method on `PendingImport` now returns `file_stats`
- [ ] B1b: Verify upload endpoint `POST /batch/upload` returns `file_stats` in the `stats` object
- [ ] B1c: Test with `curl` that upload response includes `file_stats` array

#### Task B2: Add `source_filename` and `token_count` to preview response

- [ ] B2a: Verify `ParsedItemSummary` has `source_filename` and `token_count`
- [ ] B2b: Test with `curl` that preview response includes these fields
- [ ] B2c: Run `cargo test`

---

### Phase C: Frontend — API Types & Step Restructure

#### Task C1: Update TypeScript API types

- [ ] C1a: Add `token_count`, `source_filename` to `BatchImportItem` interface in `frontend/src/api/client.ts`
- [ ] C1b: Add `FileStat` interface: `{ filename: string; item_count: number; char_count: number; token_count: number }`
- [ ] C1c: Add `file_stats: FileStat[]` and `total_token_count: number` to `BatchImportStats`
- [ ] C1d: Run `vitest run` — verify existing tests still pass

**File:** `frontend/src/api/client.ts`

```typescript
export interface FileStat {
  filename: string
  item_count: number
  char_count: number
  token_count: number
}

export interface BatchImportStats {
  total_items: number
  valid_items: number
  error_items: number
  total_chars: number
  total_token_count: number  // NEW
  file_stats: FileStat[]      // NEW
  created_at: string
  expires_at: string
}

export interface BatchImportItem {
  index: number
  text: string
  text_preview: string
  voice: string | null
  model: string | null
  title: string | null
  context: string | null
  custom_title: string | null
  char_count: number
  token_count: number         // NEW (no frontend estimation)
  source_filename: string | null  // NEW
  has_error: boolean
  error: string | null
}
```

#### Task C2: Restructure wizard steps in `BatchImportWizard.vue`

- [ ] C2a: Update `steps` array labels: `['上传文件', '分组默认配置', '自定义任务', '提交任务']`
- [ ] C2b: Update `DialogDescription` for each step
- [ ] C2c: Remove `batch_size` from `submitConfig` (dead field, not sent to backend)

**Step configuration:**

```typescript
const steps = [
  { title: '上传文件' },        // Step 0: Upload + file stats table
  { title: '分组默认配置' },     // Step 1: Group defaults
  { title: '自定义任务' },      // Step 2: Per-item overrides (virtual list)
  { title: '提交任务' },        // Step 3: Confirm + submit
]
```

#### Task C3: Refactor Step 0 — Upload + File Stats Table

- [ ] C3a: Add `fileStats` ref: `const fileStats = ref<FileStat[]>([])`
- [ ] C3b: On successful upload, populate `fileStats` from `response.stats.file_stats`
- [ ] C3c: After "完成" badge area, add file stats table when `fileStats.length > 0`

**File stats table template:**

```html
<!-- File Stats Table -->
<div v-if="fileStats.length > 0" class="mt-4">
  <div class="text-sm font-medium mb-2">文件统计</div>
  <div class="border rounded-lg overflow-hidden">
    <table class="w-full text-sm">
      <thead class="bg-muted/50">
        <tr>
          <th class="px-3 py-2 text-left cursor-pointer hover:bg-muted" @click="sortFileStats('filename')">
            文件名 <span v-if="fileSortKey === 'filename'">{{ fileSortDir === 'asc' ? '↑' : '↓' }}</span>
          </th>
          <th class="px-3 py-2 text-right cursor-pointer hover:bg-muted" @click="sortFileStats('item_count')">
            任务数 <span v-if="fileSortKey === 'item_count'">{{ fileSortDir === 'asc' ? '↑' : '↓' }}</span>
          </th>
          <th class="px-3 py-2 text-right cursor-pointer hover:bg-muted" @click="sortFileStats('char_count')">
            字符数 <span v-if="fileSortKey === 'char_count'">{{ fileSortDir === 'asc' ? '↑' : '↓' }}</span>
          </th>
          <th class="px-3 py-2 text-right cursor-pointer hover:bg-muted" @click="sortFileStats('token_count')">
            Token数 <span v-if="fileSortKey === 'token_count'">{{ fileSortDir === 'asc' ? '↑' : '↓' }}</span>
          </th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="stat in sortedFileStats" :key="stat.filename" class="border-t hover:bg-muted/30">
          <td class="px-3 py-2 truncate max-w-[200px]">{{ stat.filename }}</td>
          <td class="px-3 py-2 text-right">{{ stat.item_count }}</td>
          <td class="px-3 py-2 text-right">{{ stat.char_count.toLocaleString() }}</td>
          <td class="px-3 py-2 text-right">{{ stat.token_count.toLocaleString() }}</td>
        </tr>
      </tbody>
      <tfoot class="bg-muted/30 font-medium">
        <tr>
          <td class="px-3 py-2">合计</td>
          <td class="px-3 py-2 text-right">{{ totalItemCount }}</td>
          <td class="px-3 py-2 text-right">{{ totalCharCount.toLocaleString() }}</td>
          <td class="px-3 py-2 text-right">{{ totalTokenCount.toLocaleString() }}</td>
        </tr>
      </tfoot>
    </table>
  </div>
</div>
```

- [ ] C3d: Add sort state + computed:

```typescript
const fileSortKey = ref<keyof FileStat>('filename')
const fileSortDir = ref<'asc' | 'desc'>('asc')

function sortFileStats(key: keyof FileStat) {
  if (fileSortKey.value === key) {
    fileSortDir.value = fileSortDir.value === 'asc' ? 'desc' : 'asc'
  } else {
    fileSortKey.value = key
    fileSortDir.value = 'asc'
  }
}

const sortedFileStats = computed(() => {
  const key = fileSortKey.value
  const dir = fileSortDir.value === 'asc' ? 1 : -1
  return [...fileStats.value].sort((a, b) => {
    const av = a[key]
    const bv = b[key]
    if (typeof av === 'string' && typeof bv === 'string') return av.localeCompare(bv) * dir
    return ((av as number) - (bv as number)) * dir
  })
})

const totalItemCount = computed(() => fileStats.value.reduce((s, f) => s + f.item_count, 0))
const totalCharCount = computed(() => fileStats.value.reduce((s, f) => s + f.char_count, 0))
const totalTokenCount = computed(() => fileStats.value.reduce((s, f) => s + f.token_count, 0))
```

**Note:** Sorting is on frontend-displayed `fileStats` from backend — this is NOT computation, just UI sort for a typically <100 rows table. All *numbers* come from backend.

- [ ] C3e: Populate `fileStats` in `uploadFile` and `uploadFolderFiles` success handlers:

```typescript
// In upload success handler:
const resp = await api.uploadBatchFile(formData)
importToken.value = resp.token
totalCount.value = resp.stats.total_items
fileStats.value = resp.stats.file_stats ?? []
uploadState.value = 'success'
```

#### Task C4: Refactor Step 1 — Group Defaults Form

- [ ] C4a: Replace current "预览编辑" step (step 1) content with group defaults form
- [ ] C4b: Fields: group_name (text input), default_voice (select, required), default_model (select), default_context (textarea)
- [ ] C4c: Show summary stats from import: total items, total chars, total tokens (from stats response)
- [ ] C4d: Validate `default_voice` is non-empty before allowing next step

**Step 1 template:**

```html
<!-- Step 1: Group Defaults -->
<div v-if="currentStep === 1" class="flex flex-col gap-6 flex-1 min-h-0 max-w-2xl mx-auto w-full">
  <!-- Summary Stats -->
  <div class="grid grid-cols-3 gap-4">
    <div class="text-center p-3 bg-muted/30 rounded-lg">
      <div class="text-2xl font-bold">{{ totalCount }}</div>
      <div class="text-xs text-muted-foreground">任务数</div>
    </div>
    <div class="text-center p-3 bg-muted/30 rounded-lg">
      <div class="text-2xl font-bold">{{ totalChars }}</div>
      <div class="text-xs text-muted-foreground">字符数</div>
    </div>
    <div class="text-center p-3 bg-muted/30 rounded-lg">
      <div class="text-2xl font-bold">{{ totalTokens }}</div>
      <div class="text-xs text-muted-foreground">Token数</div>
    </div>
  </div>

  <!-- Group Defaults Form -->
  <div class="space-y-4">
    <div>
      <Label>分组名称</Label>
      <Input v-model="submitConfig.group_name" placeholder="可选，留空使用文件名" />
    </div>
    <div>
      <Label class="text-red-500">*语音</Label>
      <Select v-model="submitConfig.default_voice">
        <SelectTrigger><SelectValue placeholder="选择语音" /></SelectTrigger>
        <SelectContent>
          <SelectItem v-for="v in voices" :key="v.id" :value="v.id">{{ v.name }}</SelectItem>
        </SelectContent>
      </Select>
    </div>
    <div>
      <Label>模型</Label>
      <Select v-model="submitConfig.default_model">
        <SelectTrigger><SelectValue placeholder="选择模型" /></SelectTrigger>
        <SelectContent>
          <SelectItem value="mimo-v2.5-tts">Mimo V2.5</SelectItem>
          <SelectItem value="default">Default</SelectItem>
        </SelectContent>
      </Select>
    </div>
    <div>
      <Label>上下文</Label>
      <Textarea v-model="submitConfig.default_context" placeholder="可选，应用于所有任务的默认上下文" rows="3" />
    </div>
  </div>
</div>
```

- [ ] C4e: Add `totalChars` and `totalTokens` computed properties:

```typescript
const totalChars = computed(() => uploadStats.value?.total_chars ?? 0)
const totalTokens = computed(() => uploadStats.value?.total_token_count ?? 0)
const uploadStats = ref<BatchImportStats | null>(null)
```

- [ ] C4f: Store stats on upload success: `uploadStats.value = resp.stats`

#### Task C5: Refactor Step 2 — Custom Tasks (Virtual List with Overrides)

- [ ] C5a: Keep existing virtual-scroll list from current step 1
- [ ] C5b: Move it to step 2 (was step 1)
- [ ] C5c: Add `source_filename` column to each item row (show filename source)
- [ ] C5d: Add `token_count` display per item
- [ ] C5e: Pre-populate edit fields with current item values (voice, model, context, title)
- [ ] C5f: Show "使用分组默认值" indicator when item values match group defaults
- [ ] C5g: Load page 0 automatically on entering step 2, clear `allItems` and `loadedPageSet`
- [ ] C5h: Keep infinite scroll `onScroll` + `loadNextPage` pattern

#### Task C6: Refactor Step 3 — Submit Confirmation

- [ ] C6a: Show summary: group name, default voice, total items, total chars/tokens
- [ ] C6b: Submit button with loading state
- [ ] C6c: On success → show success message with group_id
- [ ] C6d: On error → show `submitError` message (already implemented)
- [ ] C6e: Remove separate "完成" step — just show success inline after submit

---

### Phase D: Frontend — Tests Update

#### Task D1: Update existing tests for new step structure

- [ ] D1a: Update step transition tests (step 0→1→2→3)
- [ ] D1b: Add test for `fileStats` population from upload response
- [ ] D1c: Add test for sorting `sortedFileStats` computed
- [ ] D1d: Update preview item assertions to include `source_filename` and `token_count`
- [ ] D1e: Run `vitest run` — all tests pass

**File:** `frontend/src/components/__tests__/BatchImportWizard.import.test.ts`

---

### Phase E: Verification

- [ ] E1: Run `cargo test` — all backend tests pass
- [ ] E2: Run `vitest run` — all frontend tests pass
- [ ] E3: `cargo build` — compiles cleanly
- [ ] E4: Start backend + frontend, manually test:
  - Upload single .txt file → see file stats table with correct filename/char_count/token_count
  - Upload folder → see file stats table with per-file stats
  - Navigate Step 1, set group defaults
  - Navigate Step 2, see virtual list with source_filename and token_count per item
  - Navigate Step 3, submit successfully
- [ ] E5: LSP diagnostics clean on all changed files

---

## Execution Order

**Wave 1 (backend, must land first):** A1→A2→A3→A4 → B1→B2
**Wave 2 (frontend, depends on Wave 1 API changes):** C1→C2→C3→C4→C5→C6
**Wave 3 (tests + verification, depends on Wave 2):** D1 → E1→E2→E3→E4→E5

Backend changes MUST be deployed before frontend changes will work (the API response shape changes).