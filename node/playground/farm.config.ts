import { defineConfig } from '@farmfe/core'
import monacoEditorEsmPlugin from 'vite-plugin-monaco-editor-esm'

export default defineConfig({
    compilation: {
        presetEnv: true,
        input: {
            index: './src/index.html'
        },
        output: {
            path: 'dist',
            publicPath: '/',
            targetEnv: 'browser'
        },
        define: {
            // This is for some reason not handled by Farm correctly
            NODE_DEBUG_NATIVE: ''
        },
        // This leads to a Farm panic.
        // Enabling persistentCache in general leads to 2x slower builds
        // persistentCache: {
        //     buildDependencies: [
        //         '@fervid/napi-wasm32-wasi',
        //         path.resolve('node_modules', '@fervid', 'napi-wasm32-wasi', 'fervid.wasm32-wasi.wasm')
        //     ]
        // }
        persistentCache: false
    },
    vitePlugins: [monacoEditorEsmPlugin()],
})
