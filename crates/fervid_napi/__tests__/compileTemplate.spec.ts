import { test, expect } from 'vitest'
import { Compiler, FervidJsCompilerOptions, FervidJsCompilerOptionsTemplate } from '..'

// Spec: https://github.com/vuejs/core/blob/532cfae34996676846bf511e1f0f0bf963186e7b/packages/compiler-sfc/__tests__/compileTemplate.spec.ts#L423-L430

interface CompileOptions {
  source: string
  filename: string
  transformAssetUrls?: FervidJsCompilerOptionsTemplate['transformAssetUrls']
  // TODO: Support this option and un-skip the tests using it
  ssr?: boolean
}

function compile(opts: CompileOptions) {
  const source = `<template>${opts.source}</template>`

  const compilerOptions: FervidJsCompilerOptions = {
    template: {
      transformAssetUrls: opts.transformAssetUrls
    }
  }

  const compiler = new Compiler(compilerOptions)
  const result = compiler.compileSync(source, {
    filename: opts.filename,
    id: ''
  })

  return result
}

test('should work', () => {
  const source = `<div><p>{{ render }}</p></div>`

  const result = compile({ filename: 'example.vue', source })

  expect(result.errors.length).toBe(0)
  // expect(result.source).toBe(source)
  // should expose render fn
  // expect(result.code).toMatch(`export function render(`)
  expect(result.code).toMatch(`render (_ctx`)

  expect(result.code).toMatchSnapshot()
})

test('transform asset url options', () => {
  const input = { source: `<foo bar="~baz"/>`, filename: 'example.vue' }
  // Object option
  const { code: code1 } = compile({
    ...input,
    transformAssetUrls: {
      tags: { foo: ['bar'] },
    },
  })
  expect(code1).toMatch(`import _imports_0 from "baz";\n`)

  // NOTE: Legacy option is not supported in Fervid
  // legacy object option (direct tags config)
  // const { code: code2 } = compile({
  //   ...input,
  //   transformAssetUrls: {
  //     foo: ['bar'],
  //   },
  // })
  // expect(code2).toMatch(`import _imports_0 from 'baz'\n`)

  // false option
  const { code: code3 } = compile({
    ...input,
    transformAssetUrls: false,
  })
  expect(code3).not.toMatch(`import _imports_0 from "baz";\n`)
})

// #3447
test.skip('should generate the correct imports expression', () => {
  const { code } = compile({
    filename: 'example.vue',
    source: `
      <img src="./foo.svg"/>
      <Comp>
        <img src="./bar.svg"/>
      </Comp>
    `,
    ssr: true,
  })
  expect(code).toMatch(`_ssrRenderAttr(\"src\", _imports_1)`)
  expect(code).toMatch(`_createVNode(\"img\", { src: _imports_1 })`)
})

// #3874
test.skip('should not hoist srcset URLs in SSR mode', () => {
  const { code } = compile({
    filename: 'example.vue',
    source: `
    <picture>
      <source srcset="./img/foo.svg"/>
      <img src="./img/foo.svg"/>
    </picture>
    <router-link>
      <picture>
        <source srcset="./img/bar.svg"/>
        <img src="./img/bar.svg"/>
      </picture>
    </router-link>
    `,
    ssr: true,
  })
  expect(code).toMatchSnapshot()
})

// #6742
test('dynamic v-on + static v-on should merged', () => {
  const source = `<input @blur="onBlur" @[validateEvent]="onValidateEvent">`

  const result = compile({ filename: 'example.vue', source })

  expect(result.code).toMatchSnapshot()
})
