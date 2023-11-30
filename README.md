<div style="text-align: center">
<img src="logo.png" width="280" height="120">
</div>

# fervid
All-In-One Vue compiler written in Rust.

Currently in early development, and the closest goal is to reach feature-parity with the current [Vue SFC compiler](https://sfc.vuejs.org).

## Progress till MVP ![](https://geps.dev/progress/80)
A minimal target of this project includes:
- Vue 3 code generation;
- [unplugin](https://github.com/unjs/unplugin) integration;
- Dev/prod mode support;
- `<script setup>` support;
- Example Vue project with configuration;
- Performance comparison.

## Is it correct?
This project uses [Vue SFC playground](https://sfc.vuejs.org) as its reference to compare the output.
As of November 2023, fervid is capable of producing the DEV and PROD code almost identical to the official compiler, with some differences in:
- Context variables. This includes usages like `{{ foo + bar.buzz }}` or `<div v-if="isShown">`.
  These usages require a JavaScript parser and transformer like SWC and support for them in fervid is almost complete.
- [WIP] Patch flags. These are used to help Vue runtime when diffing the VNodes. If a VNode only has one prop which is dynamic, and all the other props and text are static, this needs to be conveyed to Vue for fast updates. I am currently researching how they are originally implemented.

To check correctness of fervid, you can compare the [playground output](https://phoenix-ru.github.io/fervid/) to the output of [official compiler](https://play.vuejs.org).

Please note that "correctness" of output will depend on the version of Vue, as Vue team may change the output and/or behaviour of the compiler. This is a big challenge for fervid.

## Is it fast?
Yes, it is incredibly fast. In fact, below is a benchmark run for a [test component](crates/fervid/benches/fixtures/input.vue).

```
  @vue/compiler-sfc:
    954 ops/s, ±1.15%     | slowest, 98.42% slower

  @fervid/napi sync:
    6 464 ops/s, ±0.08%   | 89.29% slower

  @fervid/napi async (4 threads):
    11 624 ops/s, ±2.12%  | 80.73% slower

  @fervid/napi async CPUS (23 threads):
    60 329 ops/s, ±0.67%  | fastest
```

<!-- 
| Action                     | Mean time    |
|----------------------------|--------------|
| Parsing                    | 5.58µs       |
| Code generation: CSR + DEV | 16.26µs      | -->

> Note: results are for AMD Ryzen 9 7900X running on Fedora 38 with kernel version 6.5.9

<!-- Micro-benchmarking has been done using Criterion, code for benchmarks can be found in `benches` directory. -->
Benchmarking in Node.js has been done using [`benny`](https://github.com/caderek/benny), slightly modified to take `libuv` threads into consideration.
[Source code for a benchmark](crates/fervid_napi/benchmark/bench.ts).

Better benchmarking is a TODO and has a lower priority compared to feature-completeness and usability in real-world scenarios, so **Pull Requests are welcome**.

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
- [x] JS/TS imports analysis (powered by swc_ecma_parser)
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
    - [x] v-memo
    - [x] v-once
    - [x] v-pre
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
  - [x] Return statement: inline vs render function;
  - [x] defineProps
  - [x] defineEmits
  - [x] defineExpose
  - [x] defineOptions
  - [x] defineSlots
  - [x] defineModel
  - [ ] Tests
- [x] DEV/PROD mode
- [x] Hot Module Replacement (HMR)
- [ ] Vue 2.7 support
- [ ] SSR with inline critical CSS support
- [ ] Eager pre-compilation of Vue imports (avoid unneccessary bundler->compiler calls)

Integrations
- [x] WASM binary (unpublished)
- [x] NAPI binary [@fervid/napi](https://www.npmjs.com/package/@fervid/napi)
- [x] [unplugin](https://github.com/unjs/unplugin) (in progress)
- [ ] [Farm](https://github.com/farm-fe/farm) native plugin
- [ ] [Turbopack](https://github.com/vercel/turbo) plugin (when plugin system is defined)
