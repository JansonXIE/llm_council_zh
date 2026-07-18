import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, '.', '');

  return {
    plugins: [react()],
    clearScreen: false,
    server: {
      host: env.TAURI_DEV_HOST || '127.0.0.1',
      port: 5173,
      strictPort: true,
    },
    build: {
      minify: env.TAURI_DEBUG ? false : 'esbuild',
      sourcemap: !!env.TAURI_DEBUG,
      target: env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    },
  };
});
