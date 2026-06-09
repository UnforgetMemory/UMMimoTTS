import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 1,
  workers: process.env.CI ? 1 : undefined,
  reporter: [
    ['html'],
    ['list'],
    process.env.CI ? ['github'] : ['line'],
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
      testIgnore: /full-chain\.spec\.ts|large-scale-text\.spec\.ts/,
    },
    {
      name: 'full-chain',
      testMatch: /full-chain\.spec\.ts/,
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'large-scale',
      testMatch: /large-scale-text\.spec\.ts/,
      use: { ...devices['Desktop Chrome'] },
      timeout: 60000,
    },
  ],

  webServer: {
    command: 'npm run dev',
    url: 'http://localhost:30232',
    cwd: './frontend',
    reuseExistingServer: !process.env.CI,
    timeout: 120 * 1000,
  },
});
