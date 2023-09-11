import init, { compile_sync } from '../pkg/fervid_wasm.js'
import * as monaco from 'https://cdn.jsdelivr.net/npm/monaco-editor@0.41.0/esm/vs/editor/editor.main.js/+esm'

const INITIAL =
    `<template>
    <div>
        Hello {{ name }}!
    </div>
</template>

<script>
export default {
    data: () => ({
        name: 'fervid'
    })
}
</script>
`

/** @type {HTMLElement} */
const timeEl = document.getElementById('time')
let isTimeInitial = true

function compileAndTime (input) {
    const start = performance.now()
    const result = compile_sync(input)
    const end = performance.now()

    timeEl.textContent = `${((end - start) * 1000).toFixed(0)}µs ${isTimeInitial ? '(cold)' : ''}`
    isTimeInitial = false
    return result
}

function mountEditor (inputElement, outputElement, initialValue, compile) {
    const inputEditorInstance = monaco.editor.create(inputElement, {
        value: initialValue,
        language: 'vue',
        minimap: { enabled: false }
    })

    const outputEditorInstance = monaco.editor.create(outputElement, {
        value: compile(initialValue),
        language: 'javascript',
        readOnly: true,
        minimap: { enabled: false }
    })

    // self.MonacoEnvironment = {
    //     async getWorker(_, label) {
    //         if (label === 'vue') {
    //             const worker = new vueWorker()
    //             const init = new Promise((resolve) => {
    //                 worker.addEventListener('message', (data) => {
    //                     if (data.data === 'inited') {
    //                         resolve()
    //                     }
    //                 })
    //                 worker.postMessage({
    //                     event: 'init',
    //                     tsVersion: store.state.typescriptVersion,
    //                     tsLocale: store.state.typescriptLocale || store.state.locale,
    //                 })
    //             })
    //             await init
    //             return worker
    //         }
    //         return new editorWorker()
    //     },
    // }
    monaco.languages.register({ id: 'vue', extensions: ['.vue'] })
    monaco.languages.register({ id: 'javascript', extensions: ['.js'] })
    monaco.languages.register({ id: 'typescript', extensions: ['.ts'] })

    inputEditorInstance.onDidChangeModelContent(() => {
        outputEditorInstance.setValue(compileAndTime(inputEditorInstance.getValue()))
    })
}

init().then(() => {
    mountEditor(document.getElementById('editor'), document.getElementById('output'), INITIAL, compileAndTime)
})
