import init, { compile_sync } from '../pkg/fervid_wasm.js'

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

/** @type {HTMLTextAreaElement} */
const inputEl = document.getElementById('in')
/** @type {HTMLTextAreaElement} */
const outputEl = document.getElementById('out')
/** @type {HTMLElement} */
const timeEl = document.getElementById('time')

inputEl.value = INITIAL

function compileAndOutput() {
    const inputData = inputEl.value
    if (inputData.trim().length === 0) return

    const start = performance.now()
    const result = compile_sync(inputData)
    const end = performance.now()

    outputEl.value = result
    timeEl.textContent = `${end - start}ms`
}

inputEl.addEventListener('input', compileAndOutput)

init().then(() => {
    compileAndOutput()
})
