import { describe, test, expect } from 'vitest'
import { parse as babelParse } from '@babel/parser'
import { Compiler, FervidCompileOptions } from '..'

const mockId = 'xxxxxxxx'

describe('SFC analyze <script> bindings', () => {
// https://github.com/vuejs/core/blob/272ab9fbdcb1af0535108b9f888e80d612f9171d/packages/compiler-sfc/__tests__/compileScript.spec.ts#L1252-L1306
  describe('auto name inference', () => {
    test('basic', () => {
      const { content } = compile(
        `<script setup>const a = 1</script>
          <template>{{ a }}</template>`,
        {
          filename: 'FooBar.vue',
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
        },
      )
      expect(content).not.toMatch(`name: 'FooBar'`)
      expect(content).toMatch(`name: 'Baz'`)
      assertCode(content)
    })
  })
})

describe('SFC genDefaultAs', () => {
  test('normal <script> only', () => {
    const { content } = compile(
      `<script>
      export default {}
      </script>`,
      {
        genDefaultAs: '_sfc_',
      },
    )
    expect(content).not.toMatch('export default')
    expect(content).toMatch(`const _sfc_ = {\n    __name: "anonymous"\n}`)
    assertCode(content)
  })

  test('normal <script> w/ cssVars', () => {
    const { content } = compile(
      `<script>
      export default {}
      </script>
      <style>
      .foo { color: v-bind(x) }
      </style>`,
      {
        genDefaultAs: '_sfc_',
      },
    )
    expect(content).not.toMatch('export default')
    expect(content).not.toMatch('__default__')
    expect(content).toMatch(`const _sfc_ = {\n    __name: "anonymous"\n}`)
    assertCode(content)
  })

  test('<script> + <script setup>', () => {
    const { content } = compile(
      `<script>
      export default {}
      </script>
      <script setup>
      const a = 1
      </script>`,
      {
        genDefaultAs: '_sfc_',
      },
    )
    expect(content).not.toMatch('export default')
    expect(content).toMatch(
      // Fervid never produces `Object.assign` because it assumes code downleveling is done by a bundler
      // `const _sfc_ = /*#__PURE__*/Object.assign(__default__`,
      `const _sfc_ = {`,
    )
    assertCode(content)
  })

  test('<script setup> only', () => {
    const { content } = compile(
      `<script setup>
      const a = 1
      </script>`,
      {
        genDefaultAs: '_sfc_',
      },
    )
    expect(content).not.toMatch('export default')
    expect(content).toMatch(`const _sfc_ = {\n    __name: "anonymous",\n    setup`)
    assertCode(content)
  })

  test('<script setup> only w/ ts', () => {
    const { content } = compile(
      `<script setup lang="ts">
      const a = 1
      </script>`,
      {
        genDefaultAs: '_sfc_',
      },
    )
    expect(content).not.toMatch('export default')
    // TODO https://github.com/phoenix-ru/fervid/issues/23
    // expect(content).toMatch(`const _sfc_ = /*#__PURE__*/_defineComponent(`)
    expect(content).toMatch(`const _sfc_ = `)
    assertCode(content)
  })

  test('<script> + <script setup> w/ ts', () => {
    const { content } = compile(
      `<script lang="ts">
      export default {}
      </script>
      <script setup lang="ts">
      const a = 1
      </script>`,
      {
        genDefaultAs: '_sfc_',
      },
    )
    expect(content).not.toMatch('export default')
    expect(content).toMatch(
      // TODO https://github.com/phoenix-ru/fervid/issues/23
      // There is no need for spreading, because Fervid merges trivial objects
      // `const _sfc_ = /*#__PURE__*/_defineComponent({\n  ...__default__`,
      `const _sfc_ = {\n    __name:`,
    )
    assertCode(content)
  })

  // TODO implement TS-only macros
  // This is tested in Rust side
  // test('binding type for edge cases', () => {
  //   const { bindings } = compile(
  //     `<script setup lang="ts">
  //     import { toRef } from 'vue'
  //     const props = defineProps<{foo: string}>()
  //     const foo = toRef(() => props.foo)
  //     </script>`,
  //   )
  //   expect(bindings).toStrictEqual({
  //     toRef: BindingTypes.SETUP_CONST,
  //     props: BindingTypes.SETUP_REACTIVE_CONST,
  //     foo: BindingTypes.SETUP_REF,
  //   })
  // })

  // describe('parser plugins', () => {
    // Compiler never throws (only during panic)
    // test('import attributes', () => {
    //   const { content } = compile(`
    //     <script setup>
    //     import { foo } from './foo.js' with { type: 'foobar' }
    //     </script>
    //   `)
    //   assertCode(content)

    //   expect(() =>
    //     compile(`
    //   <script setup>
    //     import { foo } from './foo.js' assert { type: 'foobar' }
    //     </script>`),
    //   ).toThrow()
    // })

    // This is not supported
    // test('import attributes (user override for deprecated syntax)', () => {
    //   const { content } = compile(
    //     `
    //     <script setup>
    //     import { foo } from './foo.js' assert { type: 'foobar' }
    //     </script>
    //   `,
    //     {
    //       babelParserPlugins: [
    //         ['importAttributes', { deprecatedAssertSyntax: true }],
    //       ],
    //     },
    //   )
    //   assertCode(content)
    // })
  // })
})

// https://github.com/vuejs/core/blob/272ab9fbdcb1af0535108b9f888e80d612f9171d/packages/compiler-sfc/__tests__/utils.ts#L11-L24
function compile(src: string, options?: Partial<FervidCompileOptions>) {
  const normalizedOptions: FervidCompileOptions = {
    filename: 'anonymous.vue',
    id: mockId,
    ...options,
  }

  const compiler = new Compiler()
  const result = compiler.compileSync(src, normalizedOptions)

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
