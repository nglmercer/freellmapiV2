import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { resolve } from 'node:path'

export default defineConfig(({ mode }) => {
  const configDir = import.meta.dirname
  const env = loadEnv(mode, resolve(configDir, '..'), '')
  const serverPort = env.PORT ?? process.env.PORT ?? 3001

  return {
    plugins: [react(), tailwindcss()],
    base: process.env.VITE_BASE ?? '/',
    envDir: resolve(configDir, '..'),
    define: {
      __SERVER_PORT__: JSON.stringify(String(serverPort)),
    },
    resolve: {
      alias: {
        '@': resolve(configDir, './src'),
      },
    },
    server: {
      proxy: {
        '/api': `http://localhost:${serverPort}`,
        '/v1': `http://localhost:${serverPort}`,
      },
    },
  }
})
