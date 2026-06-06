import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Served from https://himanshu64.github.io/himsha-network/ (GitHub Pages project site).
export default defineConfig({
  base: '/himsha-network/',
  plugins: [react()],
})
