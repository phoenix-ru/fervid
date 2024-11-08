import { expect } from 'vitest'
import { parse as babelParse } from '@babel/parser'
import { Compiler, FervidCompileOptions } from '..'

const mockId = 'xxxxxxxx'

// https://github.com/vuejs/core/blob/272ab9fbdcb1af0535108b9f888e80d612f9171d/packages/compiler-sfc/__tests__/utils.ts#L11-L24
export function compile(src: string, options?: Partial<FervidCompileOptions>, logErrors = false) {
  const normalizedOptions: FervidCompileOptions = {
    filename: 'anonymous.vue',
    id: mockId,
    ...options,
  }

  const compiler = new Compiler()
  const result = compiler.compileSync(src, normalizedOptions)

  if (result.errors.length && logErrors) {
    console.warn(result.errors[0])
  }
  
  return {
    content: result.code,
    errors: result.errors,
    bindings: result.setupBindings,
  }
}

export function assertCode(code: string) {
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
