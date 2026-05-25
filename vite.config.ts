import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Tauri 在 Windows 上探测 dev server 时，localhost 的 IPv4/IPv6 解析常常
// 与 Vite 8 默认绑定不一致，导致 "Waiting for your frontend dev server..."
// 卡住。统一用 127.0.0.1 强制 IPv4，与 tauri.conf.json::devUrl 保持一致。
const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  plugins: [react()],
  // Tauri v2 需要固定端口 + 固定 host
  server: {
    host: host || '127.0.0.1',
    port: 5173,
    strictPort: true,
    // 不监听 src-tauri/ 变更，避免 Rust 端重启时前端也重启
    watch: {
      ignored: ['**/src-tauri/**'],
    },
    hmr: host
      ? { protocol: 'ws', host, port: 1421 }
      : undefined,
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
