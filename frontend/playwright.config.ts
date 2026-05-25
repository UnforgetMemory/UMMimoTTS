import { defineConfig, devices } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: [
    ['html', { outputFolder: 'e2e-report' }],
    ['list'],
  ],

  use: {
    baseURL: 'http://localhost:30232',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],

  // Start Vite dev server (backend proxy handled by vite.config.ts)
  webServer: {
    command: 'npx vite',
    port: 30232,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
})
