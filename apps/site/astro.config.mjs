import { defineConfig } from "astro/config";
import { fileURLToPath } from "node:url";
import { demoLocalization } from "./src/demo/localize.mjs";

export default defineConfig({
  output: "static",
  site: "https://verisilo.qiu.works",
  trailingSlash: "always",
  build: { inlineStylesheets: "never" },
  vite: {
    plugins: [demoLocalization()],
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
