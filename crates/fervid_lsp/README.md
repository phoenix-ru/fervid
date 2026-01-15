## fervid_lsp crate (experimental)

Install (very early phase, currently only from within cloned repository):
```sh
cargo install --path crates/fervid_lsp --locked
```

Use in Neovim:
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
