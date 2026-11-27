import { readFileSync } from 'node:fs';
import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { svelteTesting } from '@testing-library/svelte/vite';
import { VitePWA } from 'vite-plugin-pwa';

const host = process.env.TAURI_DEV_HOST;
const appVersion = /^version\s*=\s*"([^"]+)"/m.exec(readFileSync('../Cargo.toml', 'utf8'))?.[1] ?? '0.0.0';

export default defineConfig(({ mode }) => ({
  plugins: [
    tailwindcss(),
    svelte(),
    svelteTesting(),
    ...(mode === 'web'
      ? [
          VitePWA({
            registerType: 'autoUpdate',
            includeAssets: ['favicon.svg', 'favicon.ico', 'apple-touch-icon-180x180.png'],
            manifest: {
              name: 'μChat',
              short_name: 'μChat',
              description: 'Voz e transmissões do μChat',
              lang: 'pt-BR',
              display: 'standalone',
              background_color: '#12242a',
              theme_color: '#12242a',
              icons: [
                { src: 'pwa-64x64.png', sizes: '64x64', type: 'image/png' },
                { src: 'pwa-192x192.png', sizes: '192x192', type: 'image/png' },
                { src: 'pwa-512x512.png', sizes: '512x512', type: 'image/png' },
                { src: 'maskable-icon-512x512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
              ],
            },
            workbox: { globPatterns: ['**/*.{js,css,html,svg,png,ico,woff2}'], navigateFallback: 'index.html' },
          }),
        ]
      : []),
  ],
  clearScreen: false,
  define: { __APP_VERSION__: JSON.stringify(appVersion) },
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 5174 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: {
    target: 'es2022',
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
    setupFiles: ['./src/test/setup.ts'],
    restoreMocks: true,
  },
}));
