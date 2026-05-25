import { type Locator, type Page } from '@playwright/test'

/**
 * Page Object for the Batch Import Wizard dialog.
 */
export class BatchImportPage {
  readonly page: Page

  /** Trigger button to open the import dialog */
  readonly openButton: Locator

  /** The wizard dialog itself */
  readonly dialog: Locator

  /** Hidden file input element */
  readonly fileInput: Locator

  /** Drop zone area */
  readonly dropZone: Locator

  constructor(page: Page) {
    this.page = page
    this.openButton = page.getByRole('button', { name: /批量导入|导入文件/i })
    this.dialog = page.getByRole('dialog').or(page.getByText('批量导入向导'))
    this.fileInput = page.locator('input[type="file"]')
    this.dropZone = page.locator('[data-testid="drop-zone"]').or(page.getByText('拖拽文件到此处'))
  }

  /** Navigate to the app (assumes running on baseURL) */
  async goto() {
    await this.page.goto('/')
  }

  /** Open the batch import wizard dialog */
  async openWizard() {
    await this.openButton.click()
    await this.dialog.waitFor({ state: 'visible', timeout: 5000 })
  }

  /** Upload a single .txt file via the file input */
  async uploadSingleFile(
    content: string,
    fileName = 'test-items.txt',
  ) {
    const filePayload = Buffer.from(content, 'utf-8')
    await this.fileInput.setInputFiles({
      name: fileName,
      mimeType: 'text/plain',
      buffer: filePayload,
    })
  }

  /** Upload multiple files (simulates folder upload) */
  async uploadMultipleFiles(files: { name: string; content: string }[]) {
    const filePayloads = files.map((f) => ({
      name: f.name,
      mimeType: 'text/plain' as const,
      buffer: Buffer.from(f.content, 'utf-8'),
    }))
    await this.fileInput.setInputFiles(filePayloads)
  }

  /** Wait for the upload to complete (success indicator visible) */
  async waitForUploadSuccess(timeout = 15000) {
    await this.page.waitForSelector('text=上传成功', { timeout })
  }

  /** Wait for upload error state */
  async waitForUploadError(timeout = 15000) {
    await this.page.waitForSelector('text=重试', { timeout })
  }

  /**
   * Fill the submit configuration in step 2.
   * Relies on visible Select / Input elements (shadcn-vue).
   */
  async fillSubmitConfig(overrides: {
    default_voice?: string
    default_model?: string
    batch_size?: number
  }) {
    if (overrides.default_voice) {
      const voiceSelect = this.page.getByLabel(/默认音色|voice/i)
      await voiceSelect.fill(overrides.default_voice)
    }
    if (overrides.default_model) {
      const modelSelect = this.page.getByLabel(/模型|model/i)
      await modelSelect.fill(overrides.default_model)
    }
    if (overrides.batch_size !== undefined) {
      const batchInput = this.page.locator('input[type="number"]').first()
      await batchInput.fill(String(overrides.batch_size))
    }
  }

  /** Click the submit / create-tasks button */
  async submitImport() {
    await this.page.getByRole('button', { name: /创建任务/i }).click()
  }

  /** Wait for the completion state (step 3 - done) */
  async waitForCompletion(timeout = 15000) {
    await this.page.waitForSelector('text=导入完成', { timeout })
  }
}
