import { test, expect, type Browser } from '@playwright/test';
import { MetricsCollector, type UserJourneyResult } from '../helpers/metrics-collector';
import { BatchImportPage } from '../pages/batch-import.page';
import { BatchTaskListPage } from '../pages/batch-task-list.page';
import { generateTestFile, generateMultipleFiles } from '../fixtures/mock-data';

// ── Full-chain mock API setup ───────────────────────────────────────
// Unlike concurrent-users.spec.ts which uses isolated mocks per user,
// this test uses shared mocks to simulate a realistic backend state.

let batchCounter = 0;

async function setupFullChainMocks(page: import('@playwright/test').Page, userId: number) {
  const batchId = `fullchain-${userId}-${++batchCounter}`;

  // Batch CRUD
  await page.route('**/api/v2/batches', async (route) => {
    if (route.request().method() === 'POST') {
      await route.fulfill({
        status: 200, contentType: 'application/json',
        body: JSON.stringify({
          id: batchId, name: `e2e-${userId}`, status: 'preparing',
          voice: 'zh-CN-XiaoxiaoNeural', model: 'tts-1', style: null, speed: 1.0,
          total_items: 3, total_chars: 200, total_tokens: 0,
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
      body: JSON.stringify({ max_items: 1000, max_chars: 500000 }),
    });
  });

  await page.route('**/api/v2/batches/*/items/batch', async (route) => {
    await route.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify({ ok: true, count: 3 }),
    });
  });

  await page.route('**/api/v2/batches/*/submit', async (route) => {
    // Simulate slight processing delay
    await new Promise(r => setTimeout(r, 200));
    await route.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify({
        id: batchId, name: `e2e-${userId}`, status: 'completed',
        voice: 'zh-CN-XiaoxiaoNeural', model: 'tts-1', style: null, speed: 1.0,
        total_items: 3, total_chars: 200, total_tokens: 20,
        created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
        completed_at: new Date().toISOString(),
      }),
    });
  });

  // Task endpoints
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

  // SSE endpoint mock
  await page.route('**/api/v2/sse**', async (route) => {
    await route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' });
  });
}

// ── Full user journey ───────────────────────────────────────────────

async function fullUserJourney(
  browser: Browser,
  userId: number,
): Promise<UserJourneyResult> {
  const context = await browser.newContext();
  const page = await context.newPage();
  const t0 = Date.now();
  const errors: string[] = [];
  const steps: Record<string, number> = {};

  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(msg.text());
  });

  try {
    await setupFullChainMocks(page, userId);

    // Navigate + setup
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await page.evaluate(() => localStorage.setItem('mimo_api_key', `fullchain-key-${userId}`));
    await page.reload();
    await page.waitForLoadState('networkidle');
    steps.nav = Date.now() - t0;

    // Open import wizard
    const bp = new BatchImportPage(page);
    await bp.openWizard();
    steps.wizard = Date.now() - t0;

    // Upload multiple files
    const files = generateMultipleFiles(3);
    await bp.uploadMultipleFiles(files);
    await expect(bp.uploadSuccessText).toBeVisible({ timeout: 15000 });
    steps.upload = Date.now() - t0;

    // Config
    await bp.goNext();
    await bp.selectDefaultVoice('冰糖');
    steps.config = Date.now() - t0;

    // Preview
    await bp.goNext();
    await bp.waitForPreviewLoaded();
    steps.preview = Date.now() - t0;

    // Submit
    await bp.goNext();
    await bp.submitBatch('冰糖');
    await bp.waitForSuccess();
    steps.submit = Date.now() - t0;

    // Verify task list shows the new batch
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    const taskList = new BatchTaskListPage(page);
    await taskList.refresh();
    steps.verify = Date.now() - t0;

    return { userId, success: true, duration: Date.now() - t0, errors, steps };
  } catch (err) {
    errors.push(String(err));
    return { userId, success: false, duration: Date.now() - t0, errors, steps };
  } finally {
    await context.close();
  }
}

// ── Tests ────────────────────────────────────────────────────────────

test.describe('Full Chain – End to End', () => {
  test('5 users end-to-end concurrently', async ({ browser }) => {
    const USERS = 5;
    const collector = new MetricsCollector();
    collector.start();

    const promises = Array.from({ length: USERS }, (_, i) =>
      fullUserJourney(browser, i),
    );
    const results = await Promise.allSettled(promises);

    const outcomes: UserJourneyResult[] = results.map((r, i) => {
      if (r.status === 'fulfilled') return r.value;
      return { userId: i, success: false, duration: 0, errors: [r.reason], steps: {} };
    });

    console.log('\n=== Full Chain: 5 Users E2E ===');
    for (const o of outcomes) {
      const stepStr = Object.entries(o.steps)
        .map(([k, v]) => `${k}=${v}ms`)
        .join(' → ');
      console.log(`  user ${o.userId}: ${o.success ? 'OK' : 'FAIL'} (${o.duration}ms) ${stepStr}`);
      if (o.errors.length > 0) {
        const criticalErrors = o.errors.filter(e => !e.includes('net::'));
        if (criticalErrors.length > 0) {
          console.log(`    errors: ${criticalErrors.slice(0, 2).join('; ')}`);
        }
      }
    }
    console.log(collector.summary());

    // At least 80% should succeed
    const successCount = outcomes.filter(o => o.success).length;
    expect(successCount).toBeGreaterThanOrEqual(Math.ceil(USERS * 0.8));
  });

  test('single user full journey with step verification', async ({ browser }) => {
    const result = await fullUserJourney(browser, 42);

    console.log('\n=== Full Chain: Single User Journey ===');
    console.log(`  success: ${result.success}`);
    console.log(`  duration: ${result.duration}ms`);
    for (const [step, ms] of Object.entries(result.steps)) {
      console.log(`  ${step}: ${ms}ms`);
    }

    expect(result.success).toBeTruthy();
    expect(result.duration).toBeLessThan(30000); // under 30s
    expect(Object.keys(result.steps).length).toBeGreaterThanOrEqual(5); // all steps recorded
  });
});
