import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts', 'tests/**/*.test.js'],
    globals: true,
    testTimeout: 30_000,
    // WASM_PATH is set by CI to the artifact path from the contract-build job.
    // Tests that require the WASM file read this env var and skip gracefully
    // when it is absent (local dev without a full Soroban build).
    env: {
      WASM_PATH: process.env['WASM_PATH'] ?? '',
    },
  },
});
