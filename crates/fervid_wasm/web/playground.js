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

// DOM
const inputElement = document.getElementById('editor'),
    outputElement = document.getElementById('output'),
    /** @type {HTMLElement} */
    outputTimeElement = document.getElementById('time'),
    /** @type {HTMLButtonElement} */
    prodToggleButton = document.getElementById('prod-toggle')

let inputEditorInstance,
    outputEditorInstance,
    value = INITIAL,
    isTimeInitial = true,
    is_prod = true

function mountEditor () {
    inputEditorInstance = monaco.editor.create(inputElement, {
        value,
        language: 'html',
        minimap: { enabled: false }
    })

    outputEditorInstance = monaco.editor.create(outputElement, {
        value: compileAndTime(),
        language: 'javascript',
        readOnly: true,
        minimap: { enabled: false }
    })

    monaco.languages.register({ id: 'vue', extensions: ['.vue'] })
    monaco.languages.register({ id: 'javascript', extensions: ['.js'] })
    monaco.languages.register({ id: 'typescript', extensions: ['.ts'] })

    function recompile () {
        value = inputEditorInstance.getValue()
        outputEditorInstance.setValue(compileAndTime())
    }

    inputEditorInstance.onDidChangeModelContent(recompile)

    prodToggleButton.onclick = () => {
        is_prod = !is_prod
        prodToggleButton.classList.toggle('prod')
        prodToggleButton.textContent = is_prod ? 'PROD' : 'DEV'
        recompile()
    }
}

function compileAndTime () {
    const start = performance.now()
    const result = compile_sync(value, is_prod)
    const end = performance.now()

    outputTimeElement.textContent = `${((end - start) * 1000).toFixed(0)}µs ${isTimeInitial ? '(cold)' : ''}`
    isTimeInitial = false
    return result
}

init().then(mountEditor)
