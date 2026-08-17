import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  build: {
    outDir: 'dist',
  },
  server: {
    proxy: {
      // The Monitor's self-signed cert would otherwise fail `npm run dev`'s
      // separate origin; `secure: false` only affects this local proxy.
      '/api': {
        target: 'https://127.0.0.1:8443',
        changeOrigin: true,
        secure: false,
        ws: true,
      },
    },
  },
})
