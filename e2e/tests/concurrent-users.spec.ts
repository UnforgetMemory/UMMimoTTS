import { test, expect, type Browser } from '@playwright/test';
import { MetricsCollector, type UserJourneyResult } from '../helpers/metrics-collector';
import { BatchImportPage } from '../pages/batch-import.page';
import { BatchTaskListPage } from '../pages/batch-task-list.page';
import { generateTestFile } from '../fixtures/mock-data';

// ── Mock API setup (same pattern as batch-import.spec.ts) ─────────────

const MOCK_BATCH_ID_PREFIX = 'mock-batch-';

async function setupApiMocks(page: import('@playwright/test').Page, userId: number) {
  const batchId = `${MOCK_BATCH_ID_PREFIX}${userId}-${Date.now()}`;

  await page.route('**/api/v2/batches', async (route) => {
    if (route.request().method() === 'POST') {
      await route.fulfill({
        status: 200, contentType: 'application/json',
        body: JSON.stringify({
          id: batchId, name: `cu-${userId}`, status: 'preparing',
          voice: 'zh-CN-XiaoxiaoNeural', model: 'tts-1', style: null, speed: 1.0,
          total_items: 3, total_chars: 100, total_tokens: 0,
          created_at: new Date().toISOString(), updated_at: new Date().toISOString(), completed_at: null,
        }),
      });
    } else {
      await route.fulfill({
        status: 200, contentType: 'application/json', body: JSON.stringify([]),
      });
    }
  });

  await page.route('**/api/v2/batches/*/items/batch', async (route) => {
    await route.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify({ ok: true, count: 3 }),
    });
  });

  await page.route('**/api/v2/batches/*/submit', async (route) => {
    await route.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify({
        id: batchId, name: `cu-${userId}`, status: 'completed',
        voice: 'zh-CN-XiaoxiaoNeural', model: 'tts-1', style: null, speed: 1.0,
        total_items: 3, total_chars: 100, total_tokens: 10,
        created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
        completed_at: new Date().toISOString(),
      }),
    });
  });

  await page.route('**/api/v2/tasks**', async (route) => {
    await route.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify({ data: [], total: 0, page: 0, page_size: 50 }),
    });
  });

  await page.route('**/api/v2/groups**', async (route) => {
    await route.fulfill({
      status: 200, contentType: 'application/json', body: JSON.stringify([]),
    });
  });
}

// ── Simulate a single user journey ──────────────────────────────────

async function simulateUserJourney(
  browser: Browser,
  userId: number,
): Promise<UserJourneyResult> {
  const context = await browser.newContext();
  const page = await context.newPage();
  const t0 = Date.now();
  const errors: string[] = [];
  const steps: Record<string, number> = {};

  // Capture console errors
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(msg.text());
  });

  try {
    // Setup mocks
    await setupApiMocks(page, userId);

    // Step 1: Navigate
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await page.evaluate(() => localStorage.setItem('mimo_api_key', `mock-key-${userId}`));
    await page.reload();
    await page.waitForLoadState('networkidle');
    steps.goto = Date.now() - t0;

    // Step 2: Open wizard
    const bp = new BatchImportPage(page);
    await bp.openWizard();
    steps.openWizard = Date.now() - t0;

    // Step 3: Upload file
    const testFile = generateTestFile(`user-${userId}.txt`);
    await bp.uploadSingleFile(testFile);
    await expect(bp.uploadSuccessText).toBeVisible({ timeout: 15000 });
    steps.upload = Date.now() - t0;

    // Step 4: Config
    await bp.goNext();
    await bp.selectDefaultVoice('冰糖');
    steps.config = Date.now() - t0;

    // Step 5: Preview
    await bp.goNext();
    await bp.waitForPreviewLoaded();
    steps.preview = Date.now() - t0;

    // Step 6: Submit
    await bp.goNext();
    await bp.submitBatch('冰糖');
    await bp.waitForSuccess();
    steps.submit = Date.now() - t0;

    // Step 7: Navigate to task list
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    steps.taskList = Date.now() - t0;

    return { userId, success: true, duration: Date.now() - t0, errors, steps };
  } catch (err) {
    errors.push(String(err));
    return { userId, success: false, duration: Date.now() - t0, errors, steps };
  } finally {
    await context.close();
  }
}

// ── Tests ────────────────────────────────────────────────────────────

test.describe('Concurrent User Simulation', () => {
  test('3 users batch import concurrently', async ({ browser }) => {
    const USERS = 3;
    const collector = new MetricsCollector();
    collector.start();

    // Launch 3 concurrent user journeys
    const promises = Array.from({ length: USERS }, (_, i) =>
      simulateUserJourney(browser, i),
    );
    const results = await Promise.allSettled(promises);

    const outcomes: UserJourneyResult[] = results.map((r, i) => {
      if (r.status === 'fulfilled') return r.value;
      return { userId: i, success: false, duration: 0, errors: [r.reason], steps: {} };
    });

    // Report
    for (const o of outcomes) {
      collector.record('journey', o.duration);
      for (const e of o.errors) collector.recordError(`user ${o.userId}: ${e}`);
    }

    console.log('\n=== 3 Users Concurrent Import ===');
    for (const o of outcomes) {
      console.log(`  user ${o.userId}: ${o.success ? 'OK' : 'FAIL'} (${o.duration}ms) errors=${o.errors.length}`);
    }
    console.log(collector.summary());

    // Assertions
    const successCount = outcomes.filter(o => o.success).length;
    expect(successCount).toBeGreaterThanOrEqual(2); // at least 2/3 succeed
    expect(outcomes.every(o => o.errors.filter(e => !e.includes('net::')).length === 0 || o.success))
      .toBeTruthy();
  });

  test('5 users import + task list concurrently', async ({ browser }) => {
    const IMPORTERS = 3;
    const VIEWERS = 2;
    const collector = new MetricsCollector();
    collector.start();

    // Importers create batches
    const importerPromises = Array.from({ length: IMPORTERS }, (_, i) =>
      simulateUserJourney(browser, i),
    );

    // Viewers just navigate and check task list
    const viewerPromises = Array.from({ length: VIEWERS }, async (_, i) => {
      const context = await browser.newContext();
      const page = await context.newPage();
      const errors: string[] = [];
      page.on('console', (msg) => {
        if (msg.type() === 'error') errors.push(msg.text());
      });

      try {
        await setupApiMocks(page, 100 + i);
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        const taskList = new BatchTaskListPage(page);
        await taskList.refresh();

        // Viewer stays on page for a bit, refreshing periodically
        for (let j = 0; j < 3; j++) {
          await page.waitForTimeout(500);
          await taskList.refresh();
        }

        return { userId: 100 + i, success: true, duration: 0, errors, steps: {} as Record<string, number> };
      } catch (err) {
        errors.push(String(err));
        return { userId: 100 + i, success: false, duration: 0, errors, steps: {} as Record<string, number> };
      } finally {
        await context.close();
      }
    });

    const allResults = await Promise.allSettled([
      ...importerPromises,
      ...viewerPromises,
    ]);

    const outcomes: UserJourneyResult[] = allResults.map((r, i) => {
      if (r.status === 'fulfilled') return r.value;
      return { userId: i, success: false, duration: 0, errors: [r.reason], steps: {} };
    });

    console.log('\n=== 5 Users (3 importers + 2 viewers) ===');
    for (const o of outcomes) {
      const role = o.userId >= 100 ? 'viewer' : 'importer';
      console.log(`  ${role} ${o.userId}: ${o.success ? 'OK' : 'FAIL'} errors=${o.errors.length}`);
    }
    console.log(collector.summary());

    // Viewers should always succeed (just reading)
    const viewerOutcomes = outcomes.filter(o => o.userId >= 100);
    expect(viewerOutcomes.every(o => o.success)).toBeTruthy();
  });

  test('rapid click stress - single user', async ({ page }) => {
    await setupApiMocks(page, 99);
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await page.evaluate(() => localStorage.setItem('mimo_api_key', 'mock-key-rapid'));
    await page.reload();
    await page.waitForLoadState('networkidle');

    const errors: string[] = [];
    page.on('console', (msg) => {
      if (msg.type() === 'error') errors.push(msg.text());
    });

    // Rapidly open and close the wizard 5 times
    const bp = new BatchImportPage(page);
    for (let i = 0; i < 5; i++) {
      await bp.openWizard();
      // Verify dialog opened
      await expect(bp.dialog).toBeVisible({ timeout: 3000 });
      // Close via cancel
      if (await bp.cancelBtn.isVisible().catch(() => false)) {
        await bp.cancelBtn.click();
        await page.waitForTimeout(300);
      } else {
        // Press Escape
        await page.keyboard.press('Escape');
        await page.waitForTimeout(300);
      }
    }

    // Filter out known non-critical errors (e.g. network errors from mocks)
    const criticalErrors = errors.filter(e =>
      !e.includes('net::') && !e.includes('Failed to load resource')
    );

    console.log(`\n=== Rapid Click Stress ===`);
    console.log(`  5 open/close cycles, critical errors: ${criticalErrors.length}`);

    // No critical errors should occur
    expect(criticalErrors.length).toBe(0);
  });
});
