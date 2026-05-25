import type { Page, Locator } from '@playwright/test';
import { expect } from '@playwright/test';

export class TaskListPage {
  readonly page: Page;

  readonly sidebar: Locator;
  readonly groupCards: Locator;
  readonly taskItems: Locator;
  readonly searchInput: Locator;
  readonly refreshBtn: Locator;

  constructor(page: Page) {
    this.page = page;

    this.sidebar = page.locator('aside, [class*="sidebar"]').first();
    this.groupCards = page.locator('[class*="group"]').filter({ has: page.locator('[class*="card"]') });
    this.taskItems = page.locator('[class*="task"]');
    this.searchInput = page.getByPlaceholder(/搜索/);
    this.refreshBtn = page.getByRole('button', { name: '刷新' });
  }

  /** Navigate to the app (root URL where task list sidebar is visible) */
  async goto() {
    await this.page.goto('/');
    await this.page.waitForLoadState('networkidle');
  }

  /** Count the number of batch group cards */
  async getGroupCardCount(): Promise<number> {
    return this.groupCards.count();
  }

  /** Count the number of visible task items */
  async getTaskCount(): Promise<number> {
    return this.taskItems.count();
  }

  /** Search for a task by name */
  async searchTasks(query: string) {
    await this.searchInput.fill(query);
    // Wait a bit for the debounced search
    await this.page.waitForTimeout(500);
  }

  /** Refresh the task list */
  async refresh() {
    await this.refreshBtn.click();
    // Wait for loading to finish
    await expect(this.refreshBtn).toBeEnabled({ timeout: 10000 });
  }
}
