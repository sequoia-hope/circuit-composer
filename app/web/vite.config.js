import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Relative paths everywhere: the same build is served at /app/dist/ here, under the repository's
// path on GitHub Pages, and at /apps/composer/ through Boardroom's proxy.
export default defineConfig({
  base: './',
  plugins: [svelte()],
  build: { outDir: '../dist', emptyOutDir: true, target: 'es2022' },
  worker: { format: 'es' },
});
