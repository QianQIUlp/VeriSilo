import { defineConfig } from "astro/config";
import { fileURLToPath } from "node:url";

export default defineConfig({
  output: "static",
  site: "https://verisilo.qiu.works",
  trailingSlash: "always",
  build: { inlineStylesheets: "never" },
  vite: {
    esbuild: { jsx: "automatic" },
    resolve: {
      alias: [
        {
          find: /(?:\.\.\/|\.\/)*network-check-client\.js$/,
          replacement: fileURLToPath(
            new URL(
              "../desktop/src/preview/public-network.ts",
              import.meta.url,
            ),
          ),
        },
      ],
    },
  },
});
