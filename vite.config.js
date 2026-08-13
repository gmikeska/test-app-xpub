import { defineConfig } from "vite";
import { nodePolyfills } from "vite-plugin-node-polyfills";

// test-app-xpub is a server-rendered (Askama) Rust app. Vite is used only to
// bundle the client entry modules that import npm packages
// (`@emeraldlabs/emvault-jade`) into plain ES modules that the Rust-rendered
// pages load from `/static/dist/`. There is no Vite dev server / index.html —
// `npm run dev` rebuilds on change while `cargo run` serves the pages.
export default defineConfig({
  // `ledger-bitcoin` pulls in `bitcoinjs-lib`, which expects Node's `Buffer`
  // (and `stream`). Shim them for the browser so the Ledger bundle runs — the
  // Jade/Trezor bundles don't need this but the plugin is harmless for them.
  plugins: [
    nodePolyfills({
      include: ["buffer", "stream"],
      globals: { Buffer: true, global: true, process: true },
    }),
  ],
  build: {
    outDir: "static/dist",
    emptyOutDir: true,
    target: "es2022",
    modulePreload: false,
    rollupOptions: {
      input: {
        onboard: "client/onboard.js",
        "proposal-sign": "client/proposal-sign.js",
        ledger: "client/ledger.js",
      },
      output: {
        entryFileNames: "[name].js",
        chunkFileNames: "chunks/[name]-[hash].js",
        format: "es",
      },
    },
  },
});
