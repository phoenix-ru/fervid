import { Compiler } from '@fervid/napi-wasm32-wasi'

import * as monaco from 'monaco-editor/editor/editor.api.js';
import './monaco-features';
import 'monaco-editor/languages/definitions/html/register.js';
import 'monaco-editor/languages/definitions/css/register.js';
import 'monaco-editor/languages/definitions/typescript/register.js';
import 'monaco-editor/languages/definitions/javascript/register.js';
import 'monaco-editor/languages/features/html/register.js';
import 'monaco-editor/languages/features/typescript/register.js';

import editorWorker from 'monaco-editor/editor/editor.worker.js?worker&url';
import jsonWorker from 'monaco-editor/language/json/json.worker.js?worker&url';
import cssWorker from 'monaco-editor/language/css/css.worker.js?worker&url';
import htmlWorker from 'monaco-editor/language/html/html.worker.js?worker&url';
import tsWorker from 'monaco-editor/language/typescript/ts.worker.js?worker&url';

// @ts-ignore
self.MonacoEnvironment = {
	getWorker(_: any, label: string) {
		if (label === 'json') {
			return new Worker(jsonWorker, { type: 'module' });
		}
		if (label === 'css' || label === 'scss' || label === 'less') {
			return new Worker(cssWorker, { type: 'module' });
		}
		if (label === 'html' || label === 'handlebars' || label === 'razor') {
			return new Worker(htmlWorker, { type: 'module' });
		}
		if (label === 'typescript' || label === 'javascript') {
			return new Worker(tsWorker, { type: 'module' });
		}
		return new Worker(editorWorker, { type: 'module' });
	}
};

const INITIAL =
    `<template>
    <h1>
        Hello {{ name }}!
    </h1>
    <input v-model="name">
</template>

<script setup>
import { ref } from 'vue'

const name = ref('fervid')
</script>
`

// State
let value = INITIAL,
    isTimeInitial = true,
    isProduction = true

// Initialize the compiler
const diagnostics = { errorLinesColumns: true }
let compiler = new Compiler({ isProduction, diagnostics })
function reinitializeCompiler() {
    compiler = new Compiler({
        isProduction,
        diagnostics,
    })
}

// DOM
const inputElement = getElementById('editor'),
    outputElement = getElementById('output'),
    outputTimeElement = getElementById('time'),
    prodToggleButton = getElementById('prod-toggle')

let inputEditorInstance: monaco.editor.IStandaloneCodeEditor,
    outputEditorInstance: monaco.editor.IStandaloneCodeEditor

function mountEditor() {
    inputEditorInstance = monaco.editor.create(inputElement, {
        value,
        language: 'html',
        minimap: { enabled: false }
    })

    outputEditorInstance = monaco.editor.create(outputElement, {
        value: compileAndTime().code,
        language: 'typescript',
        readOnly: true,
        minimap: { enabled: false },
        quickSuggestions: false,
    })

    monaco.languages.register({ id: 'vue', extensions: ['.vue'] })
    monaco.languages.register({ id: 'javascript', extensions: ['.js'] })
    monaco.languages.register({ id: 'typescript', extensions: ['.ts'] })

    function recompile() {
        value = inputEditorInstance.getValue()
        const compilationResult = compileAndTime()
        outputEditorInstance.setValue(compilationResult.code)

        const errors = compilationResult.errors.map(it => (console.log(it), {
            startLineNumber: it.startLineNumber,
            endLineNumber: it.endLineNumber,
            startColumn: it.startColumn + 1,
            endColumn: it.endColumn + 1,
            severity: 8,
            message: it.message
        }))

        monaco.editor.setModelMarkers(inputEditorInstance.getModel()!, 'fervid', errors)
    }

    inputEditorInstance.onDidChangeModelContent(recompile)

    prodToggleButton.onclick = () => {
        isProduction = !isProduction
        prodToggleButton.classList.toggle('prod')
        prodToggleButton.textContent = isProduction ? 'PROD' : 'DEV'
        reinitializeCompiler()
        recompile()
    }
}

function getElementById(id: string): HTMLElement {
    const el = document.getElementById(id)
    if (el === null) {
        throw new Error(`Element not found: ${id}`)
    }
    return el
}

function compileAndTime() {
    const start = performance.now()
    const result = compiler.compileSync(value, {
        filename: 'anonymous.vue',
        id: 'xxxxxx'
    })
    const end = performance.now()

    outputTimeElement.textContent = `${((end - start) * 1000).toFixed(0)}µs ${isTimeInitial ? '(cold)' : ''}`
    isTimeInitial = false
    return result
}

mountEditor()
