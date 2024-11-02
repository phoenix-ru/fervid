import { expect, test } from 'vitest'
import { assertCode, compile } from './utils'

test('defineExpose()', () => {
  const { content } = compile(`
<script setup>
defineExpose({ foo: 123 })
</script>
`)
  assertCode(content)
  // should remove defineOptions import and call
  expect(content).not.toMatch('defineExpose')
  // should generate correct setup signature
  expect(content).toMatch(`setup (__props, { expose: __expose }) {`)
  // should replace callee
  expect(content).toMatch(`
        __expose({
            foo: 123
        })`)
})

test('<script> after <script setup> the script content not end with `\\n`', () => {
  const { content } = compile(`
  <script setup>
  import { x } from './x'
  </script>
  <script>const n = 1</script>
  `)
  assertCode(content)
})
