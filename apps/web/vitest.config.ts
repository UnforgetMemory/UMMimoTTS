import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vitest/config'

// Single jsdom environment shared by pure-function and component tests.
export default defineConfig({
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.{ts,tsx}'],
    setupFiles: ['src/test/setup.ts'],
    globals: true,
  },
})
