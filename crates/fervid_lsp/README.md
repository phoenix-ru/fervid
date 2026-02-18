## fervid_lsp crate (experimental)

`fervid_lsp` is an experimental Language Server for Vue / Nuxt projects built as a part of Fervid project.
It focuses on fast, practical editor features (completion + go-to-definition), especially around Nuxt auto-imports and components.

### Status

- ✅ Nuxt globals: completions + go-to-definition via `.nuxt/imports.d.ts` and `.nuxt/components.d.ts`
- ✅ Local analysis: uses Fervid's `transform_sfc` pipeline to collect bindings and template scopes
- ✅ Parsing and transformation error diagnostics
- ⚠️ Still evolving: better diagnostics and better `<template>` support

### Install

Install directly from GitHub (no local clone required):

```sh
cargo install --git https://github.com/phoenix-ru/fervid --locked --path crates/fervid_lsp
```

For a specific branch:

```sh
cargo install --git https://github.com/phoenix-ru/fervid --branch feat/29-implement-lsp-for-nuxt --locked --path crates/fervid_lsp
```

### Neovim setup

Use it in Neovim with this minimal config:

```lua
local capabilities = require('cmp_nvim_lsp').default_capabilities(vim.lsp.protocol.make_client_capabilities())

vim.lsp.config('fervid_lsp', {
  cmd = { "fervid_lsp" },
  capabilities = capabilities,
  filetypes = { 'vue' },
  root_markers = { "package.json", ".git" },
})
vim.lsp.enable('fervid_lsp')
```

### Usage alongside `vue_ls`

Neovim will query *all* attached LSP servers for `textDocument/definition`.
If another Vue/TS server is slow or returns empty results slowly (such as `vue_ls` on large projects), you can disable its definition capability for Vue buffers:

```lua
vim.lsp.config('vue_ls', {
  -- ... other options
  on_attach = function(client)
    -- Disable goto-definition
    client.server_capabilities.definitionProvider = false
  end,
})
```
