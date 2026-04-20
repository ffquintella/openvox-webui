import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'path';
import { fileURLToPath } from 'url';
import { readFileSync } from 'fs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Read version from package.json
const packageJson = JSON.parse(readFileSync(path.resolve(__dirname, 'package.json'), 'utf-8'));
const appVersion = packageJson.version;

export default defineConfig(({ mode }) => {
  // Load env files from project root (parent directory)
  // This allows sharing .env with the backend
  const projectRoot = path.resolve(__dirname, '..');
  const env = loadEnv(mode, projectRoot, '');

  // Also check frontend-specific env (frontend/.env takes precedence)
  const frontendEnv = loadEnv(mode, __dirname, '');

  // Merge with frontend env taking precedence
  const mergedEnv = { ...env, ...frontendEnv };

  const vitePort = parseInt(mergedEnv.VITE_PORT || '5050', 10);
  const backendPort = mergedEnv.VITE_BACKEND_PORT || '5051';
  const backendHost = mergedEnv.VITE_BACKEND_HOST || 'localhost';

  return {
    plugins: [react()],
    resolve: {
      alias: {
        '@': path.resolve(__dirname, './src'),
      },
    },
    // Expose env vars to the client (prefixed with VITE_)
    envDir: projectRoot,
    server: {
      port: vitePort,
      host: mergedEnv.VITE_HOST || 'localhost',
      proxy: {
        '/api': {
          target: `http://${backendHost}:${backendPort}`,
          changeOrigin: true,
        },
      },
    },
    build: {
      outDir: 'dist',
      sourcemap: true,
      rollupOptions: {
        output: {
          manualChunks(id) {
            if (id.includes('node_modules/react-dom') || id.includes('node_modules/react/') || id.includes('node_modules/react-router-dom')) {
              return 'vendor-react';
            }
            if (id.includes('node_modules/@tanstack/react-query')) {
              return 'vendor-query';
            }
            if (id.includes('node_modules/recharts')) {
              return 'vendor-charts';
            }
            if (id.includes('node_modules/lucide-react') || id.includes('node_modules/zustand')) {
              return 'vendor-ui';
            }
          },
        },
      },
      chunkSizeWarningLimit: 500, // Keep warning at 500KB
    },
    define: {
      __APP_VERSION__: JSON.stringify(appVersion),
    },
  };
});
