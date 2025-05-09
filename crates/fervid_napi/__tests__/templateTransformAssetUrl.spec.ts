import { describe, expect, test } from 'vitest'
import { Compiler, FervidCompileOptions, FervidTransformAssetUrlsOptions } from '..'

const mockId = 'xxxxxxxx'

function compileWithAssetUrls(
  template: string,
  options?: FervidTransformAssetUrlsOptions,
  compileOptions?: Partial<FervidCompileOptions>,
) {
  const normalizedOptions: FervidCompileOptions = {
    filename: 'anonymous.vue',
    id: mockId,
    ...compileOptions,
  }

  // Note: Fervid doesn't support partial parsing/transform/generation in its JS API,
  // thus the full `compile` is used instead
  const compiler = new Compiler({
    template: {
      transformAssetUrls: options
    }
  })

  const sfc = `<template>${template}</template>`

  return compiler.compileSync(sfc, normalizedOptions)
}

describe('compiler sfc: transform asset url', () => {
  test('transform assetUrls', () => {
    const result = compileWithAssetUrls(`
			<img src="./logo.png"/>
			<img src="~fixtures/logo.png"/>
			<img src="~/fixtures/logo.png"/>
			<img src="http://example.com/fixtures/logo.png"/>
			<img src="//example.com/fixtures/logo.png"/>
			<img src="/fixtures/logo.png"/>
			<img src="data:image/png;base64,i"/>
		`)

    expect(result.code).toMatchSnapshot()
  })

  /**
   * vuejs/component-compiler-utils#22 Support uri fragment in transformed require
   */
  test('support uri fragment', () => {
    const result = compileWithAssetUrls(
      '<use href="~@svg/file.svg#fragment"></use>' +
        '<use href="~@svg/file.svg#fragment"></use>',
      {},
      {
        // TODO
        // hoistStatic: true,
      },
    )
    expect(result.code).toMatchSnapshot()
  })

  /**
   * vuejs/component-compiler-utils#22 Support uri fragment in transformed require
   */
  test('support uri is empty', () => {
    const result = compileWithAssetUrls('<use href="~"></use>')

    expect(result.code).toMatchSnapshot()
  })

  test('with explicit base', () => {
    const { code } = compileWithAssetUrls(
      `<img src="./bar.png"></img>` + // -> /foo/bar.png
        `<img src="bar.png"></img>` + // -> bar.png (untouched)
        `<img src="~bar.png"></img>` + // -> still converts to import
        `<img src="@theme/bar.png"></img>`, // -> still converts to import
      {
        base: '/foo',
      },
    )
    expect(code).toMatch(`import _imports_0 from "bar.png"`)
    expect(code).toMatch(`import _imports_1 from "@theme/bar.png"`)
    expect(code).toMatchSnapshot()
  })

  test('with includeAbsolute: true', () => {
    const { code } = compileWithAssetUrls(
      `<img src="./bar.png"/>` +
        `<img src="/bar.png"/>` +
        `<img src="https://foo.bar/baz.png"/>` +
        `<img src="//foo.bar/baz.png"/>`,
      {
        includeAbsolute: true,
      },
    )
    expect(code).toMatchSnapshot()
  })

  // vitejs/vite#298
  test('should not transform hash fragments', () => {
    const { code } = compileWithAssetUrls(
      `<svg viewBox="0 0 10 10" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
        <defs>
          <circle id="myCircle" cx="0" cy="0" r="5" />
        </defs>
        <use x="5" y="5" xlink:href="#myCircle" />
      </svg>`,
    )
    // should not remove it
    expect(code).toMatch(`"xlink:href": "#myCircle"`)
  })

  test('should allow for full base URLs, with paths', () => {
    const { code } = compileWithAssetUrls(`<img src="./logo.png" />`, {
      base: 'http://localhost:3000/src/',
    })

    expect(code).toMatchSnapshot()
  })

  test('should allow for full base URLs, without paths', () => {
    const { code } = compileWithAssetUrls(`<img src="./logo.png" />`, {
      base: 'http://localhost:3000',
    })

    expect(code).toMatchSnapshot()
  })

  test('should allow for full base URLs, without port', () => {
    const { code } = compileWithAssetUrls(`<img src="./logo.png" />`, {
      base: 'http://localhost',
    })

    expect(code).toMatchSnapshot()
  })

  test('should allow for full base URLs, without protocol', () => {
    const { code } = compileWithAssetUrls(`<img src="./logo.png" />`, {
      base: '//localhost',
    })

    // Note: `//localhost` is not a valid URL base -> it gets transformed to `http://localhost`
    // in contrast to the official compiler
    expect(code).toMatchSnapshot()
  })

  // TODO Stringify not implemented yet
  test.skip('transform with stringify', () => {
    const { code } = compileWithAssetUrls(
      `<div>` +
        `<img src="./bar.png"/>` +
        `<img src="/bar.png"/>` +
        `<img src="https://foo.bar/baz.png"/>` +
        `<img src="//foo.bar/baz.png"/>` +
        `<img src="./bar.png"/>` +
        `</div>`,
      {
        includeAbsolute: true,
      },
      {
        // TODO
        // hoistStatic: true,
        // transformHoist: stringifyStatic,
      },
    )
    expect(code).toMatch(`_createStaticVNode`)
    expect(code).toMatchSnapshot()
  })

  test('transform with stringify with space in absolute filename', () => {
    const { code } = compileWithAssetUrls(
      `<div><img src="/foo bar.png"/></div>`,
      {
        includeAbsolute: true,
      },
      {
        // TODO
        // hoistStatic: true,
        // transformHoist: stringifyStatic,
      },
    )
    expect(code).toMatch(`_createElementVNode`)
    expect(code).toContain(`import _imports_0 from "/foo bar.png"`)
  })

  test('transform with stringify with space in relative filename', () => {
    const { code } = compileWithAssetUrls(
      `<div><img src="./foo bar.png"/></div>`,
      {
        includeAbsolute: true,
      },
      {
        // TODO
        // hoistStatic: true,
        // transformHoist: stringifyStatic,
      },
    )
    expect(code).toMatch(`_createElementVNode`)
    expect(code).toContain(`import _imports_0 from "./foo bar.png"`)
  })
})
