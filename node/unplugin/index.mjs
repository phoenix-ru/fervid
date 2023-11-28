import { createUnplugin } from 'unplugin'
import { compileSync } from '@fervid/napi'

const unplugin = createUnplugin(({ mode = 'production', hmr = false }, meta) => {
  const isProd = mode === 'production'
  const shouldAddHmr = hmr
  const bundler = meta.framework

  return {
    name: 'unplugin-fervid',

    // webpack's id filter is outside of loader logic,
    // an additional hook is needed for better perf on webpack
    transformInclude(id) {
      return id.endsWith('.vue')
    },

    // just like rollup transform
    transform(code, id) {
      const base = compileSync(code, { isProd })

      if (!shouldAddHmr) {
        return base
      }

      const hmr = bundler === 'webpack'
        ? webpackHmr(id)
        : viteHmr(id)

      return base + hmr
    }
  }
})

/** @param {string} id */
function webpackHmr (id) {
  return `
if (import.meta.webpackHot) {
  __WEBPACK_DEFAULT_EXPORT__.__hmrId = '${id}'
  const api = __VUE_HMR_RUNTIME__
  import.meta.webpackHot.accept()
  if (!api.createRecord('${id}', __WEBPACK_DEFAULT_EXPORT__)) {
    api.reload('${id}', __WEBPACK_DEFAULT_EXPORT__)
  }
  // module.hot.accept('${id}', () => {
  // api.rerender('${id}', __WEBPACK_DEFAULT_EXPORT__.render)
  // })
}`
}

// TODO This is untested
/** @param {string} id */
function viteHmr (id) {
  return `
if (import.meta.hot) {
  import.meta.hot.accept((newModule) => {
    if (!newModule) return

    const api = __VUE_HMR_RUNTIME__
    newModule.__hmrId = '${id}'

    if (!api.createRecord('${id}', newModule)) {
      api.reload('${id}', newModule)
    }
  })
}`
}

export const vitePlugin = unplugin.vite
export const rollupPlugin = unplugin.rollup
export const webpackPlugin = unplugin.webpack
export const rspackPlugin = unplugin.rspack
export const esbuildPlugin = unplugin.esbuild
