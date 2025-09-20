import { defineConfig } from '@farmfe/core'
import farmPluginWorker from '@farmfe/plugin-worker'

const PUBLIC_PATH = process.env.PUBLIC_PATH || '/'

export default defineConfig({
    compilation: {
        presetEnv: true,
        input: {
            index: './src/index.html'
        },
        output: {
            path: 'dist',
            publicPath: PUBLIC_PATH,
            targetEnv: 'browser'
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
    plugins: [
        farmPluginWorker()
    ],
})
