# Justfile for the Fervid project
#
# Uses the system default shell.
# On Windows, prefer WSL or Git Bash for compatibility.

# Show available commands with descriptions
default:
    @just --list --unsorted

# --- Rust Workspace ---

# Build all Rust crates in dev mode
build:
    cargo build --workspace

# Build all Rust crates in release mode
build-release:
    cargo build --workspace --release

# Run all tests across workspace and targets
test:
    cargo test --workspace --all-targets

# Run Clippy on all targets and deny warnings
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Format all Rust code
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Remove target and build artifacts
clean:
    cargo clean

# Run fmt-check, lint, and tests together
check-all: fmt-check lint test

# --- WASM (crates/fervid_wasm) ---

# Dev build for WebAssembly target
wasm-build:
    cd {{justfile_directory()}}/crates/fervid_wasm && wasm-pack build --target web --dev

# Release build for WebAssembly target
wasm-build-release:
    cd {{justfile_directory()}}/crates/fervid_wasm && wasm-pack build --target web

# Run the WASM playground preview server using node
wasm-serve:
    cd {{justfile_directory()}}/crates/fervid_wasm && node server.js

# --- NAPI (crates/fervid_napi) ---

# Dev build for NAPI bindings using Yarn
napi-build:
    cd {{justfile_directory()}}/crates/fervid_napi && yarn build:debug

# Release build for NAPI bindings using Yarn
napi-build-release:
    cd {{justfile_directory()}}/crates/fervid_napi && yarn build

# Run tests for the NAPI bindings
napi-test:
    cd {{justfile_directory()}}/crates/fervid_napi && yarn test

# Bump `@fervid/napi` version and stage a commit. `new_version` is a parameter of `yarn version`
napi-version new_version:
    cd {{justfile_directory()}}/crates/fervid_napi && \
    yarn version {{new_version}} && \
    yarn run version && \
    VERSION=$(node -p "require('./package.json').version") && \
    jq --arg v "$VERSION" '.optionalDependencies |= with_entries(.value = $v)' package.json > tmp && mv tmp package.json && \
    yarn && \
    git add package.json && \
    git add npm/*/package.json && \
    git add yarn.lock && \
    git add .yarn/install-state.gz

# Commit staged NAPI changes from `napi-version` with a message which would trigger CI release
napi-publish-commit:
    cd {{justfile_directory()}}/crates/fervid_napi && \
    VERSION=$(node -p "require('./package.json').version") && \
    git commit -m "@fervid/napi@$VERSION"

# --- Other ---

# Run spell check across the project using cspell
spell:
    npx --yes cspell "**" --gitignore
