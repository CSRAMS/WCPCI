import { resolve } from "node:path";

import { defineConfig } from "astro/config";
import tailwind from "@astrojs/tailwind";
import shield from "@kindspells/astro-shield";
import icon from "astro-icon";

const rootDir = new URL(".", import.meta.url).pathname;
const modulePath = resolve(rootDir, "src", "generated", "sriHashes.mjs");

// https://astro.build/config
export default defineConfig({
    build: {
        format: "file"
    },
    prefetch: false,
    compressHTML: false,
    integrations: [
        tailwind({ nesting: true }),
        icon(),
        shield({ sri: { hashesModule: modulePath } })
    ]
});
