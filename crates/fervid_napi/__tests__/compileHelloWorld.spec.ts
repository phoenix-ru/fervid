import { test, expect } from 'vitest'

import { compileSync } from '../index'

const HELLO_WORLD = `
<template>
  <div class="simple compiler input">
    Hello, {{ compilerName }}!
  </div>
</template>

<script setup>
import { ref } from 'vue'

const compilerName = ref('fervid')
</script>
`

test('should work', () => {
  expect(compileSync(HELLO_WORLD)).toMatchInlineSnapshot(`
    "import { ref } from 'vue';
    import { createElementBlock as _createElementBlock, openBlock as _openBlock, toDisplayString as _toDisplayString } from \\"vue\\";
    export default {
        setup () {
            const compilerName = ref('fervid');
            return (_ctx, _cache)=>(_openBlock(), _createElementBlock(\\"div\\", {
                    class: \\"simple compiler input\\"
                }, \\" Hello, \\" + _toDisplayString(compilerName.value) + \\"! \\"));
        }
    };
    "
  `)
})
