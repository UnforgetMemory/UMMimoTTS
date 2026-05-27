import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'
import { readFileSync } from 'fs'

// Read package.json version
const packageJson = JSON.parse(readFileSync(path.resolve(__dirname, 'package.json'), 'utf-8'))

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  define: {
    '__APP_VERSION__': JSON.stringify(packageJson.version),
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  server: {
    port: 30232,
    proxy: {
      '/api': {
        target: 'http://localhost:30231',
        changeOrigin: true,
      },
    },
  },
})
