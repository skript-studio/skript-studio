var _a;
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";
// vite-plugin-monaco-editor ships as CJS (`module.exports.default = fn`), so the
// ESM default import resolves to the namespace object under some bundlers; unwrap it.
import monacoEditorPluginPkg from "vite-plugin-monaco-editor";
var monacoEditorPlugin = (_a = monacoEditorPluginPkg
    .default) !== null && _a !== void 0 ? _a : monacoEditorPluginPkg;
// Tauri expects a fixed port; this also helps when running inside dev containers.
var host = process.env.TAURI_DEV_HOST;
// https://vitejs.dev/config/
export default defineConfig({
    plugins: [
        react(),
        // Bundle Monaco's web workers as blob URLs so they work inside Tauri's webview.
        monacoEditorPlugin({
            languageWorkers: ["editorWorkerService", "json", "typescript", "css"],
        }),
    ],
    resolve: {
        alias: {
            "@": path.resolve(__dirname, "./src"),
        },
    },
    // Vite options tailored for Tauri development; see https://v2.tauri.app/reference/config
    clearScreen: false,
    server: {
        port: 1420,
        strictPort: true,
        host: host || false,
        hmr: host
            ? { protocol: "ws", host: host, port: 1421 }
            : undefined,
        watch: {
            // Don't watch the Rust source
            ignored: ["**/src-tauri/**"],
        },
    },
    envPrefix: ["VITE_", "TAURI_"],
    build: {
        target: "esnext",
        sourcemap: true,
        minify: "esbuild",
    },
});
