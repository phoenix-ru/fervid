# rust-vue-compiler

Vue SFC compiler written in Rust.
Currently in early development, and the closest goal is to reach feature-parity with the current ![Vue SFC compiler](https://sfc.vuejs.org).

## Roadmap
Parser
- [x] Template parsing
- [ ] W3 Spec compliance

Analyzer
- [ ] Template scope construction
- [ ] Error reporting
- [ ] JS/TS imports analysis (powered by swc_ecma_parser)
- [ ] `setup`/`data`/`props` analysis

Code generator
- [ ] Basic Vue3 code generation
  - [ ] Elements
    - [x] `createElementVNode`
    - [ ] Attributes
      - [x] Static + Dynamic
      - [ ] `style` merging
      - [ ] `class` merging
    - [x] Children
  - [ ] Components
    - [ ] `createVNode`
    - [ ] Slots
  - [ ] Context-awareness (`_ctx`, `$data`, `$setup`)
  - [ ] Directives
    - [x] v-on
    - [x] v-bind
    - [ ] v-if
    - [ ] v-for
    - [ ] v-show
    - [ ] v-slot
    - [ ] Other directives (less priority)
  - [ ] Hoisting

- [ ] Processing `<style scoped>`
- [ ] `<script setup>` support
- [ ] Vue 2.7 support
- [ ] SSR with inline critical CSS support
- [ ] Eager pre-compilation of Vue imports (avoid unneccessary bundler->compiler calls)

Integrations
- [ ] WASM binary (with/without WASI)
- [ ] NAPI binary
- [ ] ![unplugin](https://github.com/unjs/unplugin)
- [ ] ![Turbopack](https://github.com/vercel/turbo) plugin (when plugin system is defined)
