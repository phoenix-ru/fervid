<div style="text-align: center">
<img src="logo.png" width="280" height="120">
</div>

# fervid

Vue SFC compiler written in Rust.
Currently in early development, and the closest goal is to reach feature-parity with the current [Vue SFC compiler](https://sfc.vuejs.org).

## Progress till MVP ![](https://geps.dev/progress/44)
A minimal target of this project includes:
- Vue 3 code generation;
- [unplugin](https://github.com/unjs/unplugin) integration;
- Dev/prod mode support;
- `<script setup>` support;
- Example Vue project with configuration;
- Performance comparison.

## Is it correct?
This project uses [Vue SFC playground](https://sfc.vuejs.org) as its reference to compare the output. As of April 2023, fervid is capable of producing the DEV code almost identical to the official compiler, except for:
- Context variables. This includes usages like `{{ foo + bar.buzz }}` or `<div v-if="isShown">`.
  These usages require a JavaScript parser and transformer like SWC and support for them in fervid is currently ongoing.
- Patch flags. These are used to help Vue runtime when diffing the VNodes. If a VNode only has one prop which is dynamic, and all the other props and text are static, this needs to be conveyed to Vue for fast updates. I am currently researching how they are originally implemented.

To check correctness of fervid, you can compare the [run log](run.log) to the output of playground. For doing so, go to https://sfc.vuejs.org and paste in the contents of [src/test/input.vue](./src/test/input.vue).

Please note that "correctness" of output will depend on the version of Vue, as Vue team may change the output and/or behaviour of the compiler. This is a big challenge for fervid.

## Is it fast?
Yes, it is incredibly fast. In fact, below are the parsing/compilation times benchmarked for a [test component](src/test/input.vue).

| Action                     | Mean time    |
|----------------------------|--------------|
| Parsing                    | 5.58µs       |
| Code generation: CSR + DEV | 16.26µs      |

> Note: results are for AMD Ryzen 9 5900HX running on Fedora 37 with kernel version 6.1.6

Micro-benchmarking has been done using Criterion, code for benchmarks can be found in `benches` directory.

Actual benchmarking is a TODO and has much lower priority compared to feature-completeness and usability in real-world scenarios, so **Pull Requests are welcome**.


## Roadmap
Parser
- [x] Template parsing
- [ ] W3 Spec compliance

Analyzer
- [x] Template scope construction
- [ ] Error reporting
- [ ] JS/TS imports analysis (powered by swc_ecma_parser)
- [ ] `setup`/`data`/`props` analysis

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
    - [x] Other directives (less priority)
  - [ ] Patch flags
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
