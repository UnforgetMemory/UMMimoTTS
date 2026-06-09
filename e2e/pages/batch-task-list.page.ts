import type { Page, Locator } from '@playwright/test';

export class BatchTaskListPage {
  readonly page: Page;

  readonly batchCards: Locator;
  readonly statusBadges: Locator;
  readonly refreshBtn: Locator;

  constructor(page: Page) {
    this.page = page;
    this.batchCards = page.locator('[data-testid="batch-card"], [class*="batch"]').filter({ has: page.locator('text=/cu-|stress|concurrent/') });
    this.statusBadges = page.locator('[class*="status"], [class*="badge"]');
    this.refreshBtn = page.getByRole('button', { name: /刷新|refresh/i });
  }

  async goto() {
    await this.page.goto('/');
    await this.page.waitForLoadState('networkidle');
  }

  async getBatchCount(): Promise<number> {
    return this.batchCards.count();
  }

  async refresh() {
    const btn = this.refreshBtn;
    if (await btn.isVisible().catch(() => false)) {
      await btn.click();
      await this.page.waitForLoadState('networkidle');
    }
  }

  /** Check if any batch shows 'completed' status */
  async hasCompletedBatch(): Promise<boolean> {
    const completed = this.page.locator('text=/完成|completed|done/i');
    return (await completed.count()) > 0;
  }
}
