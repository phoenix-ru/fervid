import { describe, test, expect } from 'vitest'
import { parse as babelParse } from '@babel/parser'
import { Compiler, FervidCompileOptions } from '..'

describe('SFC analyze <script> bindings', () => {
// https://github.com/vuejs/core/blob/272ab9fbdcb1af0535108b9f888e80d612f9171d/packages/compiler-sfc/__tests__/compileScript.spec.ts#L1252-L1306
  describe('auto name inference', () => {
    test('basic', () => {
      const { content } = compile(
        `<script setup>const a = 1</script>
          <template>{{ a }}</template>`,
        {
          filename: 'FooBar.vue',
          id: ''
        },
      )
      expect(content).toMatch(`export default {
    __name: "FooBar"`)
      assertCode(content)
    })

    test('do not overwrite manual name (object)', () => {
      const { content } = compile(
        `<script>
          export default {
            name: 'Baz'
          }
          </script>
          <script setup>const a = 1</script>
          <template>{{ a }}</template>`,
        {
          filename: 'FooBar.vue',
          id: ''
        },
      )
      expect(content).not.toMatch(`name: 'FooBar'`)
      expect(content).toMatch(`name: 'Baz'`)
      assertCode(content)
    })

    test('do not overwrite manual name (call)', () => {
      const { content } = compile(
        `<script>
          import { defineComponent } from 'vue'
          export default defineComponent({
            name: 'Baz'
          })
          </script>
          <script setup>const a = 1</script>
          <template>{{ a }}</template>`,
        {
          filename: 'FooBar.vue',
          id: ''
        },
      )
      expect(content).not.toMatch(`name: 'FooBar'`)
      expect(content).toMatch(`name: 'Baz'`)
      assertCode(content)
    })
  })
})

// https://github.com/vuejs/core/blob/272ab9fbdcb1af0535108b9f888e80d612f9171d/packages/compiler-sfc/__tests__/utils.ts#L11-L24
function compile(src: string, options: FervidCompileOptions) {
  const compiler = new Compiler()
  const result = compiler.compileSync(src, options)

  if (result.errors.length) {
    console.warn(result.errors[0])
  }

  return {
    content: result.code
  }
}

function assertCode(code: string) {
  // parse the generated code to make sure it is valid
  try {
    babelParse(code, {
      sourceType: 'module',
      plugins: [
        'typescript',
        ['importAttributes', { deprecatedAssertSyntax: true }],
      ],
    })
  } catch (e: any) {
    console.log(code)
    throw e
  }
  expect(code).toMatchSnapshot()
}
