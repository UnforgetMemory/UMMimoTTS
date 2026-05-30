import type { Page, Locator } from '@playwright/test';
import { expect } from '@playwright/test';

export class BatchImportPage {
  readonly page: Page;

  // Sidebar trigger buttons
  readonly sidebarNewBatchBtn: Locator;
  readonly collapsedNewBatchBtn: Locator;

  // Dialog elements
  readonly dialog: Locator;
  readonly dialogTitle: Locator;
  readonly dialogDescription: Locator;

  // Step indicator items
  readonly stepIndicators: Locator;

  // Upload step
  readonly dropZone: Locator;
  readonly fileInput: Locator;
  readonly uploadSuccessText: Locator;
  readonly uploadErrorText: Locator;
  readonly uploadProgress: Locator;

  // Preview step
  readonly previewInfo: Locator;
  readonly previewRows: Locator;
  readonly previewNextBtn: Locator;

  // Submit step
  readonly defaultVoiceTrigger: Locator;
  readonly defaultModelTrigger: Locator;
  readonly defaultContextTextarea: Locator;
  readonly batchSizeInput: Locator;
  readonly submitBtn: Locator;
  readonly summaryText: Locator;

  // Done step
  readonly successHeading: Locator;
  readonly successDescription: Locator;
  readonly viewTasksBtn: Locator;

  // Navigation buttons
  readonly cancelBtn: Locator;
  readonly nextBtn: Locator;
  readonly backBtn: Locator;
  readonly closeBtn: Locator;

  constructor(page: Page) {
    this.page = page;

    // Sidebar trigger buttons
    this.sidebarNewBatchBtn = page.getByRole('button', { name: '新建批量任务' });
    this.collapsedNewBatchBtn = page.locator('button[title="新建批量任务"]');

    // Dialog elements
    this.dialog = page.getByRole('dialog');
    this.dialogTitle = page.getByRole('heading', { name: '批量导入向导' });
    this.dialogDescription = this.dialog.getByRole('document');

    // Step indicators - the 4 numbered circles
    this.stepIndicators = page.locator('.flex.items-center.gap-2 >> template').first();

    // Upload step
    this.dropZone = page.locator('.border-dashed');
    this.fileInput = page.locator('input[type="file"]');
    this.uploadSuccessText = page.getByText(/上传成功/);
    this.uploadErrorText = page.locator('.text-destructive').first();
    this.uploadProgress = page.locator('[role="progressbar"]');

    // Preview step
    this.previewInfo = page.locator('.flex.items-center.justify-between.shrink-0').first();
    this.previewRows = page.locator('[class*="border-b border-border/50"]');
    this.previewNextBtn = page.getByRole('button', { name: '下一步' });

    // Submit step (step 2)
    this.defaultVoiceTrigger = page.locator('#defaultVoice');
    this.defaultModelTrigger = page.locator('#defaultModel');
    this.defaultContextTextarea = page.locator('#defaultContext');
    this.batchSizeInput = page.locator('#batchSize');
    this.submitBtn = page.getByRole('button', { name: '创建任务' });
    this.summaryText = page.locator('.bg-muted\\/30.rounded-lg');

    // Done step
    this.successHeading = page.getByRole('heading', { name: '导入成功' });
    this.successDescription = page.getByText(/成功创建了/);
    this.viewTasksBtn = page.getByRole('button', { name: '查看任务' });

    // Navigation
    this.cancelBtn = page.getByRole('button', { name: '取消' });
    this.nextBtn = page.getByRole('button', { name: '下一步' });
    this.backBtn = page.getByRole('button', { name: '上一步' });
    this.closeBtn = page.locator('button').filter({ has: page.locator('svg.lucide-x') });
  }

  // ── Navigation ──────────────────────────────────────────────────────

  /** Open the batch import wizard from the sidebar */
  async goto() {
    await this.page.goto('/');
    // Wait for the page to settle
    await this.page.waitForLoadState('networkidle');
  }

  /** Open the batch wizard dialog by expanding sidebar then clicking the button */
  async openWizard() {
    // The sidebar is collapsed by default. First expand it via the toolbar button.
    const batchListBtn = this.page.getByRole('button', { name: '批量任务列表' });
    if (await batchListBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await batchListBtn.click();
      await this.page.waitForTimeout(500);
    }

    // Now try the text button, then the icon-only button
    try {
      await this.sidebarNewBatchBtn.click({ timeout: 3000 });
    } catch {
      await this.collapsedNewBatchBtn.click({ timeout: 3000 });
    }
    await expect(this.dialog).toBeVisible({ timeout: 5000 });
  }

  // ── Upload step ─────────────────────────────────────────────────────

  /** Upload a single file via webkitdirectory input — writes to temp dir and uses setInputFiles with dir */
  async uploadSingleFile(filePayload: { name: string; mimeType: string; buffer: Buffer }) {
    const fs = await import('fs');
    const pathMod = await import('path');
    const os = await import('os');

    const tmpDir = fs.mkdtempSync(pathMod.join(os.tmpdir(), 'pw-upload-'));
    const filePath = pathMod.join(tmpDir, filePayload.name);
    fs.writeFileSync(filePath, filePayload.buffer);

    // webkitdirectory input requires a directory path
    const fileInput = this.page.locator('input[type="file"]');
    await fileInput.setInputFiles(tmpDir);

    // Cleanup after a short delay
    setTimeout(() => {
      try { fs.rmSync(tmpDir, { recursive: true, force: true }); } catch {}
    }, 3000);
  }

  /** Upload multiple files via webkitdirectory input */
  async uploadMultipleFiles(
    filePayloads: Array<{ name: string; mimeType: string; buffer: Buffer }>,
  ) {
    const fs = await import('fs');
    const pathMod = await import('path');
    const os = await import('os');

    const tmpDir = fs.mkdtempSync(pathMod.join(os.tmpdir(), 'pw-upload-'));
    for (const f of filePayloads) {
      const fp = pathMod.join(tmpDir, f.name);
      fs.writeFileSync(fp, f.buffer);
    }

    const fileInput = this.page.locator('input[type="file"]');
    await fileInput.setInputFiles(tmpDir);

    setTimeout(() => {
      try { fs.rmSync(tmpDir, { recursive: true, force: true }); } catch {}
    }, 3000);
  }

  // ── Preview step ────────────────────────────────────────────────────

  /** Get the number of visible preview row elements */
  async getPreviewRowCount(): Promise<number> {
    // Each virtual item renders as a div with class containing "border-b border-border/50"
    const rows = this.page.locator('[class*="border-b border-border/50"]');
    return rows.count();
  }

  /** Read the total count from the preview header (e.g., "已加载 5 / 10 项") */
  async getTotalCount(): Promise<number> {
    const text = await this.previewInfo.textContent();
    if (!text) return 0;
    const match = text.match(/\/(\d+)\s*项/);
    return match ? parseInt(match[1], 10) : 0;
  }

  /** Wait for preview items to load */
  async waitForPreviewLoaded() {
    await expect(this.previewInfo).toBeVisible({ timeout: 10000 });
  }

  // ── Submit step ─────────────────────────────────────────────────────

  /** Select a voice in the Group Config step (step 2) */
  async selectDefaultVoice(voiceName: string) {
    // The voice combobox is labeled "默认音色 *"
    const voiceCombo = this.page.getByRole('combobox', { name: '默认音色 *' });
    await voiceCombo.click();
    await this.page.getByRole('option', { name: voiceName }).click();
  }

  /** Click the submit button (voice/model already set in step 2) */
  async submitBatch(_voice?: string, _model?: string) {
    await this.submitBtn.click();
  }

  /** Wait for the success state (step 3) */
  async waitForSuccess() {
    await expect(this.successHeading).toBeVisible({ timeout: 15000 });
  }

  /** Check if an upload error message is visible */
  async isErrorMessageVisible(): Promise<boolean> {
    return this.uploadErrorText.isVisible();
  }

  /** Get the error message text */
  async getErrorMessage(): Promise<string> {
    return (await this.uploadErrorText.textContent()) || '';
  }

  /** Close the dialog */
  async closeDialog() {
    await this.closeBtn.click();
    await expect(this.dialog).not.toBeVisible({ timeout: 5000 });
  }

  /** Go to the next step */
  async goNext() {
    await this.nextBtn.click();
  }

  /** Go to the previous step */
  async goBack() {
    await this.backBtn.click();
  }
}
