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

# Run fmt-check, lint, spell, and tests together
check-all: fmt-check lint test spell

# --- WASM (crates/fervid_wasm) ---

# Build for WebAssembly target
wasm-build:
    cd {{justfile_directory()}}/crates/fervid_napi && \
    pnpm build:wasm && \
    pnpm napi create-npm-dirs &&\
    mkdir -p artifacts && \
    cp *.wasm artifacts/ && \
    pnpm artifacts

# Run the WASM playground preview server using node
wasm-serve:
    cd {{justfile_directory()}}/crates/fervid_wasm && node server.js

# --- NAPI (crates/fervid_napi) ---

# Dev build for NAPI bindings using PNPM
napi-build:
    cd {{justfile_directory()}}/crates/fervid_napi && pnpm build:debug

# Release build for NAPI bindings using PNPM
napi-build-release:
    cd {{justfile_directory()}}/crates/fervid_napi && pnpm build

# Run tests for the NAPI bindings
napi-test:
    cd {{justfile_directory()}}/crates/fervid_napi && pnpm test

# Bump `@fervid/napi` version and stage a commit. `new_version` is a parameter of `pnpm version`
napi-version new_version:
    cd {{justfile_directory()}}/crates/fervid_napi && \
    pnpm version {{new_version}} && \
    pnpm run version && \
    VERSION=$(node -p "require('./package.json').version") && \
    jq --arg v "$VERSION" '.optionalDependencies |= with_entries(.value = $v)' package.json > tmp && mv tmp package.json && \
    pnpm i && \
    git add package.json && \
    git add npm/*/package.json && \
    git add pnpm-lock.yaml

# Commit staged NAPI changes from `napi-version` with a message which would trigger CI release
napi-publish-commit:
    cd {{justfile_directory()}}/crates/fervid_napi && \
    VERSION=$(node -p "require('./package.json').version") && \
    git commit -m "@fervid/napi@$VERSION"

# --- Other ---

# Run spell check across the project using cspell
spell:
    npx --yes cspell "**" --gitignore
