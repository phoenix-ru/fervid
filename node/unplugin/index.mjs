import { createUnplugin } from 'unplugin'
import { compileSync } from '@fervid/napi'
import VirtualModulesPlugin from 'webpack-virtual-modules'

const unplugin = createUnplugin(({ mode = 'production', hmr = false }, meta) => {
  const isProd = mode === 'production'
  const shouldAddHmr = hmr
  const bundler = meta.framework

  /** @type {VirtualModulesPlugin | undefined} */
  let vfs = undefined

  /**
   * Adds a file to Virtual File System.
   * This is used for additional assets like `<style>`s, custom blocks, etc.
   * @param {string} id
   * @param {string} content
   */
  function addVirtualFile(id, content) {
    if (bundler === 'webpack' && vfs) {
      vfs.writeModule(id, content)
    }
  }

  return {
    name: 'unplugin-fervid',

    transformInclude(id) {
      return id.endsWith('.vue')
    },

    transform(code, id) {
      const compileResult = compileSync(code, { isProd })

      /** @type {string[]} */
      const assetImports = []
      for (const style of compileResult.styles) {
        const idx = assetImports.length

        // e.g. `input.vue` -> `input.vue.2.css`
        const newId = `${id}.${idx}.${style.lang}`
        const imported = `import '${newId}'`
        assetImports.push(imported)
        addVirtualFile(newId, style.code)
      }

      const base = assetImports.join('\n') + '\n' + compileResult.code
      if (!shouldAddHmr) {
        return base
      }

      const hmr = bundler === 'webpack'
        ? webpackHmr(id)
        : viteHmr(id)

      return base + hmr
    },

    webpack(compiler) {
      // Find a VirtualModulesPlugin or create a new one
      vfs = compiler.options.plugins.find(p => p instanceof VirtualModulesPlugin)
      if (!vfs) {
        vfs = new VirtualModulesPlugin()
        compiler.options.plugins.push(vfs)
      }
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
}`
// module.hot.accept('${id}', () => {
// api.rerender('${id}', __WEBPACK_DEFAULT_EXPORT__.render)
// })
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
