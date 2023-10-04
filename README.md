<div style="text-align: center">
<img src="logo.png" width="280" height="120">
</div>

# fervid
All-In-One Vue compiler written in Rust.

Currently in early development, and the closest goal is to reach feature-parity with the current [Vue SFC compiler](https://sfc.vuejs.org).

## Progress till MVP ![](https://geps.dev/progress/70)
A minimal target of this project includes:
- Vue 3 code generation;
- [unplugin](https://github.com/unjs/unplugin) integration;
- Dev/prod mode support;
- `<script setup>` support;
- Example Vue project with configuration;
- Performance comparison.

## Is it correct?
This project uses [Vue SFC playground](https://sfc.vuejs.org) as its reference to compare the output. As of April 2023, fervid is capable of producing the DEV code almost identical to the official compiler, except for:
- [WIP] Context variables. This includes usages like `{{ foo + bar.buzz }}` or `<div v-if="isShown">`.
  These usages require a JavaScript parser and transformer like SWC and support for them in fervid is currently ongoing.
- [WIP] Patch flags. These are used to help Vue runtime when diffing the VNodes. If a VNode only has one prop which is dynamic, and all the other props and text are static, this needs to be conveyed to Vue for fast updates. I am currently researching how they are originally implemented.

To check correctness of fervid, you can compare the [run log](run.log) to the output of playground. For doing so, go to https://sfc.vuejs.org and paste in the contents of [crates/fervid/benches/fixtures/input.vue](crates/fervid/benches/fixtures/input.vue).

Please note that "correctness" of output will depend on the version of Vue, as Vue team may change the output and/or behaviour of the compiler. This is a big challenge for fervid.

## Is it fast?
Yes, it is incredibly fast. In fact, below are the parsing/compilation times benchmarked for a [test component](crates/fervid/benches/fixtures/input.vue).

| Action                     | Mean time    |
|----------------------------|--------------|
| Parsing                    | 5.58µs       |
| Code generation: CSR + DEV | 16.26µs      |

> Note: results are for AMD Ryzen 9 5900HX running on Fedora 37 with kernel version 6.1.6

Micro-benchmarking has been done using Criterion, code for benchmarks can be found in `benches` directory.

Actual benchmarking is a TODO and has much lower priority compared to feature-completeness and usability in real-world scenarios, so **Pull Requests are welcome**.

## Crates

### `fervid` ![wip](https://badgen.net/badge/Status/In%20progress/blue)
The main crate. At the moment of writing, it is responsible for everything, starting from parsing SFC and all the way to code generation, but this is temporary. In the future, this crate will most likely be used for CLI utility and re-exports from other crates.

### `fervid_css` ![wip](https://badgen.net/badge/Status/In%20progress/blue)
Works on the `<style>` block and enables `scoped` styles, CSS Modules and Vue-specific transformations. The backbone of this crate is [lightningcss](https://github.com/parcel-bundler/lightningcss).

### `fervid_core` ![wip](https://badgen.net/badge/Status/In%20progress/blue)
The core structures and functionality shared across crates.

### `fervid_transform` ![wip](https://badgen.net/badge/Status/In%20progress/blue)
This crate is responsible for AST transformation.
Handles `<script>` and `<script setup>` analysis and transformations, along with Typescript. Based on [SWC](https://github.com/swc-project/swc) and provides fast and correct transforms without using regular expressions.

### `fervid_parser` ![wip](https://badgen.net/badge/Status/In%20progress/blue)
Parser for Vue SFC based on [swc_html_parser](https://rustdoc.swc.rs/swc_html_parser/).

### `fervid_napi` ![wip](https://badgen.net/badge/Status/In%20progress/blue)
NAPI-rs bindings for usage in Node.js.

### `fervid_deno` ![future](https://badgen.net/badge/Status/Planned/orange)
Deno bindings for usage in Deno.

### `fervid_plugin` and `fervid_plugin_api` ![future](https://badgen.net/badge/Status/Planned/orange)
These crates allow authoring plugins for `fervid` in Rust using dynamically loaded libraries (`.so`, `.dll` and `.dylib`). These plugins allow anyone to customize how a Vue SFC is parsed, optimized and code-generated.

## Roadmap
Parser
- [x] Template parsing
- [x] W3 Spec compliance

Transformer
- [x] Template scope construction
- [ ] Error reporting
- [ ] JS/TS imports analysis (powered by swc_ecma_parser)
- [x] `setup`/`data`/`props` analysis

Code generator
- [ ] Basic Vue3 code generation
  - [x] Elements
    - [x] `createElementVNode`
    - [x] Attributes
      - [x] Static + Dynamic
      - [x] `style` merging
      - [x] `class` merging
    - [x] Children
  - [x] Components
    - [x] `createVNode`
    - [x] Slots
  - [x] Context-awareness (`_ctx`, `$data`, `$setup`)
  - [x] Directives
    - [x] v-on
    - [x] v-bind
    - [x] v-if / v-else-if / v-else
    - [x] v-for
    - [x] v-show
    - [x] v-slot
    - [x] v-model
    - [x] v-cloak
    - [x] v-html
    - [ ] v-memo
    - [ ] v-once
    - [ ] v-pre
    - [x] v-text
    - [x] Custom directives
  - [x] Built-in components
    - [x] keep-alive
    - [x] component
    - [x] transition
    - [x] transition-group
    - [x] teleport
    - [x] slot
    - [x] suspense
  - [ ] Patch flags
  - [ ] Hoisting

- [x] Processing `<style scoped>`
- [ ] `<script setup>` support
  - [x] Bindings collection;
  - [ ] Return statement: inline vs render function;
  - [x] defineProps
  - [x] defineEmits
  - [x] defineExpose
  - [ ] defineOptions
  - [ ] defineSlots
  - [x] defineModel
  - [ ] Tests
- [ ] Vue 2.7 support
- [ ] SSR with inline critical CSS support
- [ ] Eager pre-compilation of Vue imports (avoid unneccessary bundler->compiler calls)

Integrations
- [x] WASM binary (unpublished)
- [x] NAPI binary (unpublished)
- [ ] [unplugin](https://github.com/unjs/unplugin)
- [ ] [Farm](https://github.com/farm-fe/farm) native plugin
- [ ] [Turbopack](https://github.com/vercel/turbo) plugin (when plugin system is defined)
