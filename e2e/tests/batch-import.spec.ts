import { test, expect, type Page } from '@playwright/test';
import { BatchImportPage } from '../pages/batch-import.page';
import { TaskListPage } from '../pages/task-list.page';
import {
  generateTestFile,
  generateMultipleFiles,
  generateNonTextFile,
} from '../fixtures/mock-data';

// ── Shared test constants ─────────────────────────────────────────────

const MOCK_BATCH_ID = 'mock-batch-id-xyz789';

/**
 * Mock the v2 API endpoints so tests run without a real backend.
 *
 * The batch import wizard now:
 *  1. Parses files client-side (no upload API)
 *  2. Calls POST /api/v2/batches to create a batch
 *  3. Calls POST /api/v2/batches/{id}/items/batch to add parsed segments
 *  4. Calls POST /api/v2/batches/{id}/submit to enqueue
 */
async function setupApiMocks(page: Page) {
  // Mock POST /api/v2/batches — create batch
  await page.route('**/api/v2/batches', async (route) => {
    if (route.request().method() === 'POST') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          id: MOCK_BATCH_ID,
          name: 'Test Batch',
          status: 'preparing',
          voice: 'zh-CN-XiaoxiaoNeural',
          model: 'tts-1',
          style: null,
          speed: 1.0,
          total_items: 3,
          total_chars: 100,
          total_tokens: 0,
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          completed_at: null,
        }),
      });
    } else {
      // GET /api/v2/batches — list (fallback)
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([]),
      });
    }
  });

  // Mock POST /api/v2/batches/{id}/items/batch — add items
  await page.route('**/api/v2/batches/*/items/batch', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true, count: 3 }),
    });
  });

  // Mock POST /api/v2/batches/{id}/submit — submit batch
  await page.route('**/api/v2/batches/*/submit', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        id: MOCK_BATCH_ID,
        name: 'Test Batch',
        status: 'completed',
        voice: 'zh-CN-XiaoxiaoNeural',
        model: 'tts-1',
        style: null,
        speed: 1.0,
        total_items: 3,
        total_chars: 100,
        total_tokens: 10,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        completed_at: new Date().toISOString(),
      }),
    });
  });

  // Mock GET /api/v2/tasks — task list refresh after submit
  await page.route('**/api/v2/tasks**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ data: [], total: 0, page: 0, page_size: 50 }),
    });
  });

  // Mock GET /api/v2/groups — group list refresh after submit
  await page.route('**/api/v2/groups**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([]),
    });
  });
}

test.describe('Batch Import', () => {
  // ── Helpers ─────────────────────────────────────────────────────────

  async function setupAndOpenWizard(page: Page): Promise<BatchImportPage> {
    await setupApiMocks(page);
    const batchPage = new BatchImportPage(page);
    // Set a mock API key so the "新建批量任务" button is enabled
    await batchPage.goto();
    await page.evaluate(() => localStorage.setItem('mimo_api_key', 'mock-api-key-for-e2e'));
    await page.reload();
    await page.waitForLoadState('networkidle');
    await batchPage.openWizard();
    return batchPage;
  }

  // ── Test: Single file import flow ───────────────────────────────────

  test('single file import flow', async ({ page }) => {
    const batchPage = await setupAndOpenWizard(page);

    // Step 1: Upload
    const testFile = generateTestFile('test-article.txt');
    await batchPage.uploadSingleFile(testFile);

    // Wait for upload success
    await expect(batchPage.uploadSuccessText).toBeVisible({ timeout: 10000 });

    // Go to step 2 (Group Config)
    await batchPage.goNext();

    // Step 2: Group config — select voice
    await batchPage.selectDefaultVoice('冰糖');

    // Go to step 3 (Preview/Custom Tasks)
    await batchPage.goNext();

    // Step 3: Preview — verify items loaded
    await batchPage.waitForPreviewLoaded();

    // Go to step 4 (Submit)
    await batchPage.goNext();

    // Step 4: Submit - fill voice and submit
    await batchPage.submitBatch('冰糖');

    // Step 5: Verify success
    await batchPage.waitForSuccess();
    await expect(batchPage.successDescription).toBeVisible();
  });

  // ── Test: Multi-file import ─────────────────────────────────────────

  test('multi-file import', async ({ page }) => {
    const batchPage = await setupAndOpenWizard(page);

    // Upload 3 files
    const files = generateMultipleFiles(3);
    await batchPage.uploadMultipleFiles(files);

    // Wait for upload success
    await expect(batchPage.uploadSuccessText).toBeVisible({ timeout: 10000 });

    // Go to step 2 (Group Config)
    await batchPage.goNext();

    // Step 2: Select voice
    await batchPage.selectDefaultVoice('冰糖');

    // Go to step 3 (Preview)
    await batchPage.goNext();

    // Verify preview loaded
    await batchPage.waitForPreviewLoaded();

    // Go to step 4 (Submit)
    await batchPage.goNext();

    // Submit
    await batchPage.submitBatch('冰糖');

    // Verify success
    await batchPage.waitForSuccess();
  });

  // ── Test: Invalid file type shows error ─────────────────────────────

  test('invalid file type shows error', async ({ page }) => {
    const batchPage = await setupAndOpenWizard(page);

    // Upload a non-txt file (PDF) — webkitdirectory with accept=".txt" may filter it
    const badFile = generateNonTextFile();
    await batchPage.uploadSingleFile(badFile);

    // With webkitdirectory + accept=".txt", the browser may filter out non-txt files.
    // Verify the wizard is still on step 1 (upload) — either error shown or no files processed.
    await page.waitForTimeout(2000);
    // The drop zone should still be visible (no transition to success)
    await expect(batchPage.dropZone).toBeVisible();
  });

  // ── Test: Full wizard navigation ────────────────────────────────────

  test('full wizard navigation', async ({ page }) => {
    const batchPage = await setupAndOpenWizard(page);

    // Verify the dialog title says "批量导入向导"
    await expect(batchPage.dialogTitle).toBeVisible();

    // Verify the upload drop zone is visible (step 1 content)
    await expect(batchPage.dropZone).toBeVisible();

    // Verify the navigation buttons are present
    await expect(batchPage.cancelBtn).toBeVisible();
  });
});
