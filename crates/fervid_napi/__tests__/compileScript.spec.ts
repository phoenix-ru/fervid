import { describe, test, expect } from 'vitest'
import { assertCode, compile } from './utils'

describe('SFC compile <script setup>', () => {
  // https://github.com/vuejs/core/blob/530d9ec5f69a39246314183d942d37986c01dc46/packages/compiler-sfc/__tests__/compileScript.spec.ts#L16-L52
  test('should expose top level declarations', () => {
    const { content } = compile(`
      <script setup>
      import { x } from './x'
      let a = 1
      const b = 2
      function c() {}
      class d {}
      </script>

      <script>
      import { xx } from './x'
      let aa = 1
      const bb = 2
      function cc() {}
      class dd {}
      </script>
      `)

    expect(content).toMatch(
      `        return {
            get xx () {
                return xx;
            },
            get aa () {
                return aa;
            },
            set aa (v){
                aa = v;
            },
            bb,
            cc,
            dd,
            get x () {
                return x;
            },
            get a () {
                return a;
            },
            set a (v){
                a = v;
            },
            b,
            c,
            d
        };`,
    )

    // expect(bindings).toStrictEqual({
    //   x: BindingTypes.SETUP_MAYBE_REF,
    //   a: BindingTypes.SETUP_LET,
    //   b: BindingTypes.SETUP_CONST,
    //   c: BindingTypes.SETUP_CONST,
    //   d: BindingTypes.SETUP_CONST,
    //   xx: BindingTypes.SETUP_MAYBE_REF,
    //   aa: BindingTypes.SETUP_LET,
    //   bb: BindingTypes.LITERAL_CONST,
    //   cc: BindingTypes.SETUP_CONST,
    //   dd: BindingTypes.SETUP_CONST,
    // })
    assertCode(content)
  })

  // Difference with the original is that in TS `x` and `xx` may be types,
  // and compiler doesn't know without looking at the usage.
  // Since neither are used, they are excluded.
  test('should expose top level declarations w/ ts', () => {
    const { content } = compile(`
      <script setup lang="ts">
      import { x } from './x'
      let a = 1
      const b = 2
      function c() {}
      class d {}
      </script>

      <script lang="ts">
      import { xx } from './x'
      let aa = 1
      const bb = 2
      function cc() {}
      class dd {}
      </script>
      `)

    expect(content).toMatch(
      `        return {
            get aa () {
                return aa;
            },
            set aa (v){
                aa = v;
            },
            bb,
            cc,
            dd,
            get a () {
                return a;
            },
            set a (v){
                a = v;
            },
            b,
            c,
            d
        };`,
    )

    assertCode(content)
  })
})

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
    expect(content).toMatch(`const _sfc_ = _defineComponent({`)
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
      `const _sfc_ = _defineComponent({\n    __name:`,
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
