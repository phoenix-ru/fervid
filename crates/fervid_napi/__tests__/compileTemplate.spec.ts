import { test, expect } from 'vitest'
import { Compiler } from '..'

// Spec: https://github.com/vuejs/core/blob/532cfae34996676846bf511e1f0f0bf963186e7b/packages/compiler-sfc/__tests__/compileTemplate.spec.ts#L423-L430

interface CompileOptions {
  source: string
  filename: string
}

function compile(opts: CompileOptions) {
  const source = `<template>${opts.source}</template>`

  const compiler = new Compiler()
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

// #6742
test('dynamic v-on + static v-on should merged', () => {
  const source = `<input @blur="onBlur" @[validateEvent]="onValidateEvent">`

  const result = compile({ filename: 'example.vue', source })

  expect(result.code).toMatchSnapshot()
})
