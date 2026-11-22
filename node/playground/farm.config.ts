import { defineConfig } from '@farmfe/core'
import farmPluginWorker from '@farmfe/plugin-worker'
import { builtinModules, createRequire } from 'node:module'

const PUBLIC_PATH = process.env.PUBLIC_PATH || '/'
const require = createRequire(import.meta.url)
const browserFs = require.resolve('./src/browser-fs.ts')

export default defineConfig({
    compilation: {
        presetEnv: true,
        // Keep fs in the browser graph so its shim has script metadata, not external metadata
        externalNodeBuiltins: builtinModules.filter(name => name !== 'fs'),
        resolve: {
            alias: {
                'fs$': browserFs,
                // Farm's worker plugin resolves Monaco's bare worker URLs as package imports
                ...Object.fromEntries(['html', 'ts'].map(name => [
                    `${name}.worker.js$`,
                    require.resolve(`monaco-editor/languages/features/${name === 'ts' ? 'typescript' : name}/${name}.worker.js`)
                ]))
            }
        },
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
        farmPluginWorker({
            compilerConfig: {
                // Plugin options use native Config's alias array, not the JS config alias map
                resolve: { alias: [{ find: 'fs$', replacement: browserFs }] },
                output: { publicPath: PUBLIC_PATH }
            }
        })
    ],
})
