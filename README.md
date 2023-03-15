# rust-vue-compiler

Vue SFC compiler written in Rust.
Currently in early development, and the closest goal is to reach feature-parity with the current [Vue SFC compiler](https://sfc.vuejs.org).

## Progress till MVP ![](https://geps.dev/progress/30)
A minimal target of this project includes:
- Vue 3 code generation;
- ![unplugin](https://github.com/unjs/unplugin) integration;
- Dev/prod mode support;
- `<script setup>` support;
- Example Vue project with configuration;
- Performance comparison.

## Is it fast?
Yes, it is incredibly fast. In fact, below are the parsing/compilation times benchmarked for a [test component](src/test/input.vue).

| Action          | Average time |
|-----------------|--------------|
| Parsing         | 11.753µs     |
| Code generation | 80.733µs     |

> Note: results are for AMD Ryzen 9 5900HX running on Fedora 37 with kernel version 6.1.6

Nevertheless, at this point there is no proper benchmarking implemented and the data above comes from running the same simple snippet multiple times and averaging the results:
```rust
let n = Instant::now();
test_real_compilation();
println!("Time took: {:?}", n.elapsed());
```

Actual benchmarking is a TODO and has much lower priority compared to feature-completeness and usability in real-world scenarios, so **Pull Requests are welcome**.


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
  - [x] Components
    - [x] `createVNode`
    - [x] Slots
  - [ ] Context-awareness (`_ctx`, `$data`, `$setup`)
  - [ ] Directives
    - [x] v-on
    - [x] v-bind
    - [x] v-if / v-else-if / v-else
    - [x] v-for
    - [x] v-show
    - [x] v-slot
    - [x] v-model
    - [x] Other directives (less priority)
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
