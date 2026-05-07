import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

// `/api` is proxied to the backend in both dev (vite) and prod (nginx).
// Keep this in sync with `web/nginx.conf`.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': {
        target: process.env.BACKEND_URL ?? 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
  },
})
