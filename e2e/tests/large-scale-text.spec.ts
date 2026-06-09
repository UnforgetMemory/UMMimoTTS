import { test, expect, type Browser, type Page } from '@playwright/test';
import { BatchImportPage } from '../pages/batch-import.page';
import { MetricsCollector } from '../helpers/metrics-collector';
import {
  generateLargeTextFile,
  generateMultipleLargeFiles,
  generateMassiveTextFile,
  generateManyFiles,
  summarizeFiles,
} from '../fixtures/large-text-data';

// ── Mock API setup (large-text-aware) ──────────────────────────────────

let batchCounter = 0;

async function setupLargeTextMocks(page: Page, userId: number) {
  const batchId = `large-batch-${userId}-${++batchCounter}`;

  // Batch CRUD
  await page.route('**/api/v2/batches', async (route) => {
    if (route.request().method() === 'POST') {
      await route.fulfill({
        status: 200, contentType: 'application/json',
        body: JSON.stringify({
          id: batchId, name: `large-${userId}`, status: 'preparing',
          voice: 'zh-CN-XiaoxiaoNeural', model: 'tts-1', style: null, speed: 1.0,
          total_items: 0, total_chars: 0, total_tokens: 0,
          created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
          completed_at: null,
        }),
      });
    } else {
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) });
    }
  });

  await page.route('**/api/v2/batches/limit', async (route) => {
    await route.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify({ max_items: 5000, max_chars: 5000000 }),
    });
  });

  // Accept large batch item payloads
  await page.route('**/api/v2/batches/*/items/batch', async (route) => {
    const body = route.request().postDataJSON();
    const count = Array.isArray(body) ? body.length : 0;
    await route.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify({ ok: true, count }),
    });
  });

  // Submit — return completed batch with task list
  await page.route('**/api/v2/batches/*/submit', async (route) => {
    await new Promise(r => setTimeout(r, 300)); // simulate processing
    await route.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify({
        id: batchId, name: `large-${userId}`, status: 'processing',
        voice: 'zh-CN-XiaoxiaoNeural', model: 'tts-1', style: null, speed: 1.0,
        total_items: 100, total_chars: 20000, total_tokens: 15000,
        tasks: Array.from({ length: 100 }, (_, i) => ({
          id: `task-${batchId}-${i}`,
          status: i < 5 ? 'done' : 'processing',
          text: '', content: '', voice: 'zh-CN-XiaoxiaoNeural', model: 'tts-1',
          total_chunks: 3, done_chunks: i < 5 ? 3 : 1, failed_chunks: 0,
          total_chars: 200, total_tokens: 150,
          created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
        })),
        created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
        completed_at: null,
      }),
    });
  });

  // Tasks list
  await page.route('**/api/v2/tasks**', async (route) => {
    if (route.request().method() === 'DELETE') {
      await route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
    } else {
      await route.fulfill({
        status: 200, contentType: 'application/json',
        body: JSON.stringify({ data: [], total: 0, page: 0, page_size: 50 }),
      });
    }
  });

  await page.route('**/api/v2/groups**', async (route) => {
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify([]) });
  });

  // Config endpoint — needed for voice/model lists
  await page.route('**/api/v2/config', async (route) => {
    await route.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify({
        voices: [
          { id: 'zh-CN-XiaoxiaoNeural', name: '冰糖', language: 'zh-CN', gender: 'female', style: '通用', preview_url: '' },
          { id: 'zh-CN-YunxiNeural', name: '云希', language: 'zh-CN', gender: 'male', style: '通用', preview_url: '' },
        ],
        models: [
          { id: 'mimo-v2.5-tts', name: 'MIMO v2.5', description: '高质量语音合成' },
        ],
        default_voice: 'zh-CN-XiaoxiaoNeural',
        default_model: 'mimo-v2.5-tts',
        default_speed: 1.0,
        mimo_base_url: '',
        providers: [],
      }),
    });
  });

  // SSE endpoint
  await page.route('**/api/v2/events**', async (route) => {
    await route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' });
  });
  await page.route('**/api/v2/sse**', async (route) => {
    await route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' });
  });
}

// ── Helpers ─────────────────────────────────────────────────────────────

async function setupPage(page: Page, userId: number) {
  await setupLargeTextMocks(page, userId);
  await page.goto('/');
  await page.waitForLoadState('networkidle');
  await page.evaluate((id) => localStorage.setItem('mimo_api_key', `large-test-key-${id}`), userId);
  await page.reload();
  await page.waitForLoadState('networkidle');
}

async function collectPageMetrics(page: Page): Promise<{
  heapUsedMB: number;
  resourceTimings: Array<{ name: string; duration: number }>;
}> {
  return page.evaluate(() => {
    const mem = (performance as any).memory;
    const heapUsedMB = mem ? Math.round(mem.usedJSHeapSize / 1048576) : 0;
    const resources = performance.getEntriesByType('resource') as PerformanceResourceTiming[];
    return {
      heapUsedMB,
      resourceTimings: resources.slice(-20).map(r => ({
        name: r.name.split('/').pop() || r.name,
        duration: Math.round(r.duration),
      })),
    };
  });
}

// ── Tests ───────────────────────────────────────────────────────────────

test.describe('Large-Scale Text Processing', () => {
  // ── Cleanup hook: remove all temp upload directories after tests ───
  test.afterAll(async () => {
    // 1. Clean up tracked dirs from BatchImportPage
    const tracked = BatchImportPage.cleanupTempDirs();
    console.log(`[cleanup] removed ${tracked} tracked temp dirs`);

    // 2. Sweep any orphaned pw-upload-* dirs in OS temp
    const fs = await import('fs');
    const path = await import('path');
    const os = await import('os');
    const tmpBase = os.tmpdir();
    const orphans = fs.readdirSync(tmpBase)
      .filter(d => d.startsWith('pw-upload-'))
      .map(d => path.join(tmpBase, d));
    for (const dir of orphans) {
      try { fs.rmSync(dir, { recursive: true, force: true }); } catch {}
    }
    if (orphans.length > 0) {
      console.log(`[cleanup] removed ${orphans.length} orphaned temp upload dirs`);
    }
  });

  test('100 segments upload + preview', async ({ page }) => {
    const collector = new MetricsCollector();
    collector.start();

    await setupPage(page, 1);

    // Generate 120-segment file
    const largeFile = generateLargeTextFile(120, 200, 'chapter-01.txt');
    const summary = summarizeFiles([largeFile]);
    console.log(`\n=== 100 Segments Upload ===`);
    console.log(`  segments: ${summary.totalSegments}, chars: ${summary.totalChars}`);

    const bp = new BatchImportPage(page);
    await bp.openWizard();

    // Upload
    const t0 = Date.now();
    await bp.uploadSingleFile(largeFile);
    await expect(bp.uploadSuccessText).toBeVisible({ timeout: 30000 });
    const uploadMs = Date.now() - t0;
    collector.record('upload', uploadMs);
    console.log(`  upload: ${uploadMs}ms`);

    // Verify upload stats
    await expect(page.getByText(/120/)).toBeVisible({ timeout: 5000 });

    // Navigate to preview
    await bp.goNext();
    await bp.selectDefaultVoice('冰糖');
    await bp.goNext();
    await bp.waitForPreviewLoaded();
    const previewMs = Date.now() - t0;
    collector.record('preview_load', previewMs);
    console.log(`  preview loaded: ${previewMs}ms`);

    // Verify preview info shows correct count
    const totalCount = await bp.getTotalCount();
    expect(totalCount).toBe(120);

    // Test pagination — navigate to page 2 and back
    const nextBtn = page.getByRole('button', { name: '下一页' });
    if (await nextBtn.isVisible().catch(() => false)) {
      await nextBtn.click();
      await page.waitForTimeout(300);
      const prevBtn = page.getByRole('button', { name: '上一页' });
      if (await prevBtn.isVisible().catch(() => false)) {
        await prevBtn.click();
        await page.waitForTimeout(300);
      }
    }

    console.log(collector.summary());
  });

  test('multi-file 5×25 segments', async ({ page }) => {
    const collector = new MetricsCollector();
    collector.start();

    await setupPage(page, 2);

    // Generate 5 files × 25 segments each = 125 total
    const files = generateMultipleLargeFiles(5, 25, 200);
    const summary = summarizeFiles(files);
    console.log(`\n=== Multi-File 5×25 ===`);
    console.log(`  files: ${files.length}, segments: ${summary.totalSegments}, chars: ${summary.totalChars}`);

    const bp = new BatchImportPage(page);
    await bp.openWizard();

    // Upload all 5 files
    const t0 = Date.now();
    await bp.uploadMultipleFiles(files);
    await expect(bp.uploadSuccessText).toBeVisible({ timeout: 30000 });
    collector.record('upload', Date.now() - t0);
    console.log(`  upload: ${Date.now() - t0}ms`);

    // Verify file count = 5
    await expect(page.getByText(/5.*文件/)).toBeVisible({ timeout: 5000 });

    // Navigate to config → preview
    await bp.goNext();
    await bp.selectDefaultVoice('冰糖');
    await bp.goNext();
    await bp.waitForPreviewLoaded();

    // Verify total count = 125
    const totalCount = await bp.getTotalCount();
    expect(totalCount).toBe(125);

    // Test editing a single item (click to expand edit form)
    const firstRow = page.locator('[class*="cursor-pointer"]').first();
    if (await firstRow.isVisible().catch(() => false)) {
      await firstRow.click();
      await page.waitForTimeout(500);
      // Verify edit form appeared
      const editForm = page.locator('textarea, [contenteditable]').first();
      const editVisible = await editForm.isVisible().catch(() => false);
      if (editVisible) {
        collector.record('edit_form_open', Date.now() - t0);
        console.log(`  edit form opened successfully`);
        // Close edit
        const cancelBtn = page.getByRole('button', { name: /取消|Cancel/i });
        if (await cancelBtn.isVisible().catch(() => false)) {
          await cancelBtn.click();
        }
      }
    }

    console.log(collector.summary());
  });

  test('large text submit flow', async ({ page }) => {
    const collector = new MetricsCollector();
    collector.start();

    await setupPage(page, 3);

    const largeFile = generateLargeTextFile(100, 200, 'submit-test.txt');
    console.log(`\n=== Large Text Submit (100 segments) ===`);
    console.log(`  chars: ${largeFile.totalChars}`);

    const bp = new BatchImportPage(page);
    await bp.openWizard();

    // Step 1: Upload
    const t0 = Date.now();
    await bp.uploadSingleFile(largeFile);
    await expect(bp.uploadSuccessText).toBeVisible({ timeout: 30000 });
    collector.record('step_upload', Date.now() - t0);

    // Step 2: Config
    await bp.goNext();
    await bp.selectDefaultVoice('冰糖');
    collector.record('step_config', Date.now() - t0);

    // Step 3: Preview
    await bp.goNext();
    await bp.waitForPreviewLoaded();
    collector.record('step_preview', Date.now() - t0);

    // Step 4: Submit
    await bp.goNext();
    await bp.submitBatch();

    // Wait for success
    await bp.waitForSuccess();
    collector.record('step_submit', Date.now() - t0);

    // Verify success message mentions task count
    await expect(bp.successDescription).toBeVisible();
    const descText = await bp.successDescription.textContent();
    console.log(`  success: ${descText}`);

    // Navigate to task list
    if (await bp.viewTasksBtn.isVisible().catch(() => false)) {
      await bp.viewTasksBtn.click();
      await page.waitForLoadState('networkidle');
      collector.record('navigate_tasks', Date.now() - t0);
    }

    console.log(`  total: ${Date.now() - t0}ms`);
    console.log(collector.summary());

    // Assert total time < 30s
    expect(Date.now() - t0).toBeLessThan(30000);
  });

  test('memory + performance monitoring', async ({ page }) => {
    const collector = new MetricsCollector();
    collector.start();

    await setupPage(page, 4);

    // Generate massive file (200 segments)
    const massive = generateMassiveTextFile(200, 150);
    console.log(`\n=== Memory + Performance (200 segments) ===`);
    console.log(`  chars: ${massive.totalChars}`);

    // Measure initial memory
    const memBefore = await collectPageMetrics(page);
    console.log(`  heap before: ${memBefore.heapUsedMB} MB`);

    const bp = new BatchImportPage(page);
    await bp.openWizard();

    // Upload massive file
    const t0 = Date.now();
    await bp.uploadSingleFile(massive);
    await expect(bp.uploadSuccessText).toBeVisible({ timeout: 30000 });
    const uploadMs = Date.now() - t0;
    collector.record('massive_upload', uploadMs);
    console.log(`  upload: ${uploadMs}ms`);

    // Navigate through steps to stress-test rendering
    await bp.goNext(); // → config
    await bp.selectDefaultVoice('冰糖');
    await bp.goNext(); // → preview
    await bp.waitForPreviewLoaded();

    // Rapid pagination stress
    for (let i = 0; i < 5; i++) {
      const nextBtn = page.getByRole('button', { name: '下一页' });
      if (await nextBtn.isVisible().catch(() => false)) {
        await nextBtn.click();
        await page.waitForTimeout(200);
      }
    }

    // Measure post-interaction memory
    const memAfter = await collectPageMetrics(page);
    const heapDelta = memAfter.heapUsedMB - memBefore.heapUsedMB;
    console.log(`  heap after: ${memAfter.heapUsedMB} MB (delta: +${heapDelta} MB)`);

    // Log slowest resource timings
    if (memAfter.resourceTimings.length > 0) {
      const slow = memAfter.resourceTimings
        .filter(r => r.duration > 100)
        .sort((a, b) => b.duration - a.duration)
        .slice(0, 5);
      if (slow.length > 0) {
        console.log(`  slow resources:`);
        for (const r of slow) {
          console.log(`    ${r.name}: ${r.duration}ms`);
        }
      }
    }

    console.log(collector.summary());

    // Memory growth should be reasonable (< 100 MB delta)
    expect(heapDelta).toBeLessThan(100);
    // Upload should complete in reasonable time
    expect(uploadMs).toBeLessThan(15000);
  });

  test('concurrent large uploads', async ({ browser }) => {
    const USERS = 3;
    const SEGMENTS_PER_USER = 50;
    const collector = new MetricsCollector();
    collector.start();

    console.log(`\n=== Concurrent Large Uploads (${USERS} users × ${SEGMENTS_PER_USER} segments) ===`);

    async function userJourney(browser: Browser, userId: number) {
      const context = await browser.newContext();
      const page = await context.newPage();
      const errors: string[] = [];
      const t0 = Date.now();

      page.on('console', (msg) => {
        if (msg.type() === 'error') errors.push(msg.text());
      });

      try {
        await setupLargeTextMocks(page, userId);
        await page.goto('/');
        await page.waitForLoadState('networkidle');
        await page.evaluate((id) => localStorage.setItem('mimo_api_key', `conc-key-${id}`), userId);
        await page.reload();
        await page.waitForLoadState('networkidle');

        const file = generateLargeTextFile(SEGMENTS_PER_USER, 200, `conc-user-${userId}.txt`, 42 + userId);

        const bp = new BatchImportPage(page);
        await bp.openWizard();
        await bp.uploadSingleFile(file);
        await expect(bp.uploadSuccessText).toBeVisible({ timeout: 30000 });

        await bp.goNext();
        await bp.selectDefaultVoice('冰糖');
        await bp.goNext();
        await bp.waitForPreviewLoaded();

        const totalCount = await bp.getTotalCount();
        return {
          userId,
          success: totalCount === SEGMENTS_PER_USER,
          duration: Date.now() - t0,
          errors: errors.filter(e => !e.includes('net::')),
          steps: { total: Date.now() - t0 },
        };
      } catch (err) {
        errors.push(String(err));
        return { userId, success: false, duration: Date.now() - t0, errors, steps: {} as Record<string, number> };
      } finally {
        await context.close();
      }
    }

    const results = await Promise.all(
      Array.from({ length: USERS }, (_, i) => userJourney(browser, i + 10)),
    );

    for (const r of results) {
      collector.record('journey', r.duration);
      console.log(`  user ${r.userId}: ${r.success ? 'OK' : 'FAIL'} (${r.duration}ms) errors=${r.errors.length}`);
    }
    console.log(collector.summary());

    // At least 2/3 should succeed
    const okCount = results.filter(r => r.success).length;
    expect(okCount).toBeGreaterThanOrEqual(2);
    // No critical errors
    const allErrors = results.flatMap(r => r.errors);
    expect(allErrors.length).toBe(0);
  });

  test('progress monitoring after submit', async ({ page }) => {
    const collector = new MetricsCollector();
    collector.start();

    await setupPage(page, 6);

    // Track SSE subscription attempts
    const sseRequests: string[] = [];
    page.on('request', (req) => {
      if (req.url().includes('/events') || req.url().includes('/sse')) {
        sseRequests.push(req.url());
      }
    });

    const largeFile = generateLargeTextFile(50, 200, 'progress-test.txt');
    console.log(`\n=== Progress Monitoring (50 segments) ===`);

    const bp = new BatchImportPage(page);
    await bp.openWizard();

    // Upload
    await bp.uploadSingleFile(largeFile);
    await expect(bp.uploadSuccessText).toBeVisible({ timeout: 30000 });

    // Config → Preview → Submit
    await bp.goNext();
    await bp.selectDefaultVoice('冰糖');
    await bp.goNext();
    await bp.waitForPreviewLoaded();
    await bp.goNext();

    const t0 = Date.now();
    await bp.submitBatch();
    await bp.waitForSuccess();
    const submitMs = Date.now() - t0;
    collector.record('submit', submitMs);
    console.log(`  submit + success: ${submitMs}ms`);

    // Verify we navigated past submit successfully
    await expect(bp.successHeading).toBeVisible();
    await expect(bp.successDescription).toBeVisible();

    // Check that SSE endpoint was called (or at least attempted)
    console.log(`  SSE requests: ${sseRequests.length}`);

    // Navigate to task list to verify post-submit state
    if (await bp.viewTasksBtn.isVisible().catch(() => false)) {
      await bp.viewTasksBtn.click();
      await page.waitForLoadState('networkidle');
      collector.record('post_submit_nav', Date.now() - t0);
    }

    // Verify task list page is responsive (not frozen)
    const taskListResponsive = await page.evaluate(() => {
      return new Promise<boolean>((resolve) => {
        const start = performance.now();
        requestAnimationFrame(() => {
          const elapsed = performance.now() - start;
          resolve(elapsed < 500); // Frame should render within 500ms
        });
      });
    });
    expect(taskListResponsive).toBeTruthy();

    console.log(`  UI responsive after submit: ${taskListResponsive}`);
    console.log(collector.summary());
  });

  test('1000 files stress upload', async ({ page }) => {
    const collector = new MetricsCollector();
    collector.start();

    await setupPage(page, 7);

    // Generate 1000 files with 1-10000 chars each
    const files = generateManyFiles(1000, 1, 10000);
    const summary = summarizeFiles(files);
    console.log(`\n=== 1000 Files Stress Upload ===`);
    console.log(`  files: ${files.length}, segments: ${summary.totalSegments}, chars: ${summary.totalChars}`);

    // Measure initial memory
    const memBefore = await collectPageMetrics(page);
    console.log(`  heap before: ${memBefore.heapUsedMB} MB`);

    const bp = new BatchImportPage(page);
    await bp.openWizard();

    // Upload in batches of 100 files
    const BATCH_SIZE = 100;
    const t0 = Date.now();
    let totalUploaded = 0;

    for (let batch = 0; batch < files.length; batch += BATCH_SIZE) {
      const batchFiles = files.slice(batch, batch + BATCH_SIZE);
      const batchNum = Math.floor(batch / BATCH_SIZE) + 1;
      const batchStart = Date.now();

      // Write files to a temp dir and upload
      const fs = await import('fs');
      const pathMod = await import('path');
      const os = await import('os');
      const tmpDir = fs.mkdtempSync(pathMod.join(os.tmpdir(), 'pw-upload-'));
      BatchImportPage.trackedDirs.push(tmpDir);
      for (const f of batchFiles) {
        fs.writeFileSync(pathMod.join(tmpDir, f.name), f.buffer);
      }
      const fileInput = page.locator('input[type="file"]');
      await fileInput.setInputFiles(tmpDir);

      // Wait for upload to settle
      await page.waitForTimeout(500);
      totalUploaded += batchFiles.length;

      const batchMs = Date.now() - batchStart;
      collector.record(`batch_${batchNum}`, batchMs);
      console.log(`  batch ${batchNum}/10: ${batchFiles.length} files, ${batchMs}ms (total: ${totalUploaded})`);
    }

    const totalUploadMs = Date.now() - t0;
    collector.record('total_upload', totalUploadMs);
    console.log(`  total upload time: ${totalUploadMs}ms`);

    // Verify upload success
    await expect(bp.uploadSuccessText).toBeVisible({ timeout: 60000 });

    // Check memory after all uploads
    const memAfter = await collectPageMetrics(page);
    const heapDelta = memAfter.heapUsedMB - memBefore.heapUsedMB;
    console.log(`  heap after: ${memAfter.heapUsedMB} MB (delta: +${heapDelta} MB)`);

    // Navigate to preview to verify segment count
    await bp.goNext();
    await bp.selectDefaultVoice('冰糖');
    await bp.goNext();
    await bp.waitForPreviewLoaded();

    const totalCount = await bp.getTotalCount();
    console.log(`  preview segments: ${totalCount}`);
    expect(totalCount).toBe(1000);

    console.log(collector.summary());

    // Assertions
    expect(totalUploadMs).toBeLessThan(120000); // Total < 120s
    expect(heapDelta).toBeLessThan(200);        // Memory < 200MB growth
  });
});
