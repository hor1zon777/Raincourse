import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  // Tauri v2 需要固定端口
  server: {
    port: 5173,
    strictPort: true,
  },
  // 确保环境变量不泄漏到客户端
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    // Tauri 使用 Chromium，支持 es2021
    target: ['es2021', 'chrome100', 'safari13'],
    // 生产环境不生成 sourcemap
    sourcemap: !!process.env.TAURI_DEBUG,
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
  },
})
