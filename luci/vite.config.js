import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import luciPlugin from './vite-luci.mjs'

export default defineConfig({
  root: 'vite-app',
  plugins: [vue(), luciPlugin()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      input: 'vite-app/src/main.js',
      output: {
        entryFileNames: 'ddns-rs-app.js',
        assetFileNames: 'ddns-rs-app.css',
        format: 'iife'
      }
    }
  }
})
