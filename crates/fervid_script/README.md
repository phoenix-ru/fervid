# `fervid_script`
A set of Rust APIs for processing Vue's `<script>` and `<script setup>`.

## Roadmap
- [ ] `<script>` support
    - [ ] Top-level declarations and imports;
    - [ ] `data` bindings;
    - [ ] `props`;
    - [ ] `computed`;
    - [ ] `setup`;
    - [ ] `inject`;
    - [ ] `emits`;
    - [ ] `components`;
    - [ ] `methods`;
    - [ ] `expose`;
    - [ ] `name`;
    - [ ] `directives`;

- [ ] `<script setup>` support
    - [ ] Top-level declarations and imports;
    - [ ] Binding types (using bit-flags instead of enum);
    - [ ] Compiler macros:
        - [ ] `defineProps(...)` and `defineProps<...>()`;
        - [ ] `defineEmits`;
        - [ ] `defineExpose`;
        - [ ] `defineOptions`;
        - [ ] `defineSlots`;

- [ ] TypeScript support
    - [ ] `enum` bindings;
    - [ ] [Type-only props/emit declarations](https://vuejs.org/api/sfc-script-setup.html#type-only-props-emit-declarations);
    - [ ] DEV-mode [import usage checks](https://github.com/vuejs/core/blob/b36addd3bde07467e9ff5641bd1c2bdc3085944c/packages/compiler-sfc/__tests__/compileScript.spec.ts#L378);

- [ ] Additional features
    - [ ] `useCssVars`;
    - [ ] Top-level `await`;

- [ ] Compilation order
    - [ ] Analysis of scripts;
    - [ ] Merging scripts into an Options API object
        - [ ] Trivial field-by-field merging;
        - [ ] Non-trivial merging using `{ ...legacy, ...setup }`;
    - [ ] Attaching compiled template
        - [ ] Adding bindings `return` in `DEV` mode, then attaching a render function to the `_sfc_` object;
        - [ ] Inlining template in `PROD` mode;
    - [ ] Attaching additional information: `name`, `scope`, etc.
