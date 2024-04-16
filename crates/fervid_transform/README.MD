# fervid_transform

Part of the `fervid` project, an all-in-one Vue compiler.
This crate is responsible for the transformations in the template, such as optimizing the AST or transforming the JS/TS expressions in the AST nodes.

In the future this crate may accommodate the `<style>` transformations as well, which are currently in its own crate.

## `script`
A set of Rust APIs for processing Vue's `<script>` and `<script setup>`.

## Roadmap
- [x] `<script>` support
    - [x] Top-level declarations and imports;
    - [x] `data` bindings;
    - [x] `props`;
    - [x] `computed`;
    - [x] `setup`;
    - [x] `inject`;
    - [x] `emits`;
    - [x] `components`;
    - [x] `methods`;
    - [x] `expose`;
    - [x] `name`;
    - [x] `directives`;

- [ ] `<script setup>` support
    - [x] Top-level declarations and imports;
    - [x] Binding types (using bit-flags instead of enum);
    - [ ] Compiler macros:
        - [x] `defineProps(...)`;
        - [ ] `defineProps<...>()`;
        - [x] `defineEmits(...)`;
        - [ ] `defineEmits<...>()`;
        - [x] `defineExpose`;
        - [x] `defineOptions`;
        - [x] `defineSlots`;

- [ ] TypeScript support
    - [x] `enum` bindings;
    - [ ] [Type-only props/emit declarations](https://vuejs.org/api/sfc-script-setup.html#type-only-props-emit-declarations);
    - [ ] DEV-mode [import usage checks](https://github.com/vuejs/core/blob/b36addd3bde07467e9ff5641bd1c2bdc3085944c/packages/compiler-sfc/__tests__/compileScript.spec.ts#L378);

- [ ] Additional features
    - [ ] `useCssVars`;
    - [ ] Top-level `await`;

- [ ] Compilation order
    - [x] Analysis of scripts;
    - [x] Merging scripts into an Options API object
        - [x] Trivial field-by-field merging;
        - [x] Non-trivial merging using `{ ...legacy, ...setup }`;
    - [x] Attaching compiled template
        - [x] Adding bindings `return` in `DEV` mode, then attaching a render function to the `_sfc_` object;
        - [x] Inlining template in `PROD` mode;
    - [x] Attaching additional information: `name`, `scope`, etc.
