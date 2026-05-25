import { test, expect, type Page } from '@playwright/test';
import { BatchImportPage } from '../pages/batch-import.page';
import { TaskListPage } from '../pages/task-list.page';
import {
  generateTestFile,
  generateMultipleFiles,
  generateNonTextFile,
} from '../fixtures/mock-data';

// ── Shared test constants ─────────────────────────────────────────────

const MOCK_TOKEN = 'mock-import-token-abc123';
const MOCK_GROUP_ID = 'mock-group-id-xyz789';

/**
 * Mock the batch import API endpoints so tests run without a real backend.
 */
async function setupApiMocks(page: Page) {
  // Mock POST /api/v1/batch/upload
  await page.route('**/api/v1/batch/upload', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        token: MOCK_TOKEN,
        stats: {
          valid_items: 3,
          total_items: 3,
          skipped_items: 0,
          file_count: 1,
        },
      }),
    });
  });

  // Mock GET /api/v1/batch/preview
  await page.route('**/api/v1/batch/preview*', async (route) => {
    const url = new URL(route.request().url());
    const pageParam = parseInt(url.searchParams.get('page') || '0', 10);

    const allItems = [
      {
        index: 0,
        text_preview: '这是一段用于测试TTS语音合成的示例文本。',
        voice: null,
        model: null,
        custom_title: null,
        has_error: false,
      },
      {
        index: 1,
        text_preview: '第二段内容，包含一些中文标点符号。',
        voice: null,
        model: null,
        custom_title: null,
        has_error: false,
      },
      {
        index: 2,
        text_preview: '第三段：数字和字母混合测试。',
        voice: null,
        model: null,
        custom_title: null,
        has_error: false,
      },
    ];

    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        items: allItems,
        total: allItems.length,
        page: pageParam,
        per_page: 50,
      }),
    });
  });

  // Mock POST /api/v1/batch/extend
  await page.route('**/api/v1/batch/extend', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ status: 'ok' }),
    });
  });

  // Mock POST /api/v1/batch/submit
  await page.route('**/api/v1/batch/submit', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        group_id: MOCK_GROUP_ID,
        task_count: 3,
      }),
    });
  });

  // Mock GET /api/v1/voices to return at least one voice
  await page.route('**/api/v1/voices', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        {
          id: 'zh-CN-XiaoxiaoNeural',
          name: '晓晓',
          language: 'zh-CN',
          gender: 'Female',
          style: 'general',
        },
      ]),
    });
  });

  // Mock any task-related endpoints that might be called on load
  await page.route('**/api/v1/tasks**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([]),
    });
  });

  await page.route('**/api/v1/groups**', async (route) => {
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
    await batchPage.goto();
    await batchPage.openWizard();
    return batchPage;
  }

  // ── Test: Single file import flow ───────────────────────────────────

  test('single file import flow', async ({ page }) => {
    const batchPage = await setupAndOpenWizard(page);

    // Step 0: Upload
    const testFile = generateTestFile('test-article.txt');
    await batchPage.uploadSingleFile(testFile);

    // Wait for upload success
    await expect(batchPage.uploadSuccessText).toBeVisible({ timeout: 10000 });

    // Go to preview (step 1)
    await batchPage.goNext();

    // Step 1: Preview - verify items loaded
    await batchPage.waitForPreviewLoaded();
    const totalCount = await batchPage.getTotalCount();
    expect(totalCount).toBeGreaterThan(0);

    // Go to submit (step 2)
    await batchPage.goNext();

    // Step 2: Submit - fill voice and submit
    await batchPage.submitBatch('晓晓');

    // Step 3: Verify success
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

    // Go to preview
    await batchPage.goNext();

    // Verify preview loaded
    await batchPage.waitForPreviewLoaded();
    const totalCount = await batchPage.getTotalCount();
    expect(totalCount).toBeGreaterThan(0);

    // Go to submit
    await batchPage.goNext();

    // Submit
    await batchPage.submitBatch('晓晓');

    // Verify success
    await batchPage.waitForSuccess();
  });

  // ── Test: Invalid file type shows error ─────────────────────────────

  test('invalid file type shows error', async ({ page }) => {
    const batchPage = await setupAndOpenWizard(page);

    // Upload a non-txt file (PDF)
    const badFile = generateNonTextFile();
    await batchPage.uploadSingleFile(badFile);

    // Wait for error message
    const isError = await batchPage.isErrorMessageVisible();
    expect(isError).toBe(true);

    const errorMsg = await batchPage.getErrorMessage();
    expect(errorMsg).toContain('不支持的文件类型');
    expect(errorMsg).toContain('.pdf');
  });

  // ── Test: Full wizard navigation ────────────────────────────────────

  test('full wizard navigation', async ({ page }) => {
    const batchPage = await setupAndOpenWizard(page);

    // Verify 4 step indicators are visible
    // The step indicator numbers are rendered inside the dialog
    const stepNumbers = page.locator(
      '.flex.items-center.gap-2 .size-8.rounded-full',
    );
    await expect(stepNumbers).toHaveCount(4);

    // Verify the first step is active (step 1 should be highlighted)
    const firstStep = stepNumbers.first();
    await expect(firstStep).toBeVisible();

    // Verify the dialog title says "批量导入向导"
    await expect(batchPage.dialogTitle).toBeVisible();

    // Verify the upload drop zone is visible (step 0 content)
    await expect(batchPage.dropZone).toBeVisible();
  });
});
