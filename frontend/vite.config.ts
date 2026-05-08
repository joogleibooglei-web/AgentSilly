import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  build: {
    outDir: '../dist',
    lib: {
      entry: 'src/main.ts',
      name: 'EniWorldBuilder',
      formats: ['iife'],
      fileName: () => 'index.js',
    },
    cssCodeSplit: false,
    rollupOptions: {
      output: {
        assetFileNames: (assetInfo) => {
          if (assetInfo.name?.endsWith('.css')) {
            return 'index.css';
          }
          return assetInfo.name ?? 'asset';
        },
      },
    },
  },
});
