#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
OUTPUT_DIR=/tmp/opencode/fervid-pipeline-comparison
FIXTURE="$ROOT_DIR/crates/fervid/benches/fixtures/input.vue"
PRETTIER_VERSION=3.6.2

mkdir -p "$OUTPUT_DIR"

echo "Compiling old pipeline"
cargo run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" -p fervid -- "$FIXTURE" --mode prod \
    > "$OUTPUT_DIR/old-pipeline.js" \
    2> "$OUTPUT_DIR/old-pipeline.stderr.txt"

echo "Compiling new pipeline"
cargo run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" -p fervid --features new-pipeline -- "$FIXTURE" --mode prod \
    > "$OUTPUT_DIR/new-pipeline.js" \
    2> "$OUTPUT_DIR/new-pipeline.stderr.txt"

echo "Compiling Vue reference"
NODE_ENV=production FIXTURE="$FIXTURE" pnpm --dir "$ROOT_DIR/crates/fervid_napi" exec node -e '
const fs = require("node:fs")
const { compileScript, parse } = require("@vue/compiler-sfc")
const source = fs.readFileSync(process.env.FIXTURE, "utf8")
const { descriptor, errors } = parse(source, { filename: "anonymous.vue" })
if (errors.length) throw errors[0]
process.stdout.write(compileScript(descriptor, {
  id: "data-v-2b32dbe3",
  isProd: true,
  inlineTemplate: true,
  templateOptions: { slotted: descriptor.slotted },
}).content)
' > "$OUTPUT_DIR/vue-reference.js" \
  2> "$OUTPUT_DIR/vue-reference.stderr.txt"

echo "Formatting outputs with Prettier $PRETTIER_VERSION"
(
    cd "$OUTPUT_DIR"
    pnpm dlx "prettier@$PRETTIER_VERSION" --parser babel vue-reference.js \
        > vue-reference.pretty.js
    pnpm dlx "prettier@$PRETTIER_VERSION" --parser babel old-pipeline.js \
        > old-pipeline.pretty.js
    pnpm dlx "prettier@$PRETTIER_VERSION" --parser babel new-pipeline.js \
        > new-pipeline.pretty.js
)

write_diff() {
    local old_label=$1
    local new_label=$2
    local old_file=$3
    local new_file=$4
    local output_file=$5

    diff -u \
        --label "$old_label" \
        --label "$new_label" \
        "$old_file" \
        "$new_file" \
        > "$output_file" || {
        local status=$?
        if [[ $status -ne 1 ]]; then
            return "$status"
        fi
    }
}

echo "Writing diffs"
write_diff \
    vue-reference.pretty.js \
    new-pipeline.pretty.js \
    "$OUTPUT_DIR/vue-reference.pretty.js" \
    "$OUTPUT_DIR/new-pipeline.pretty.js" \
    "$OUTPUT_DIR/vue-vs-new.pretty.diff"

write_diff \
    old-pipeline.pretty.js \
    new-pipeline.pretty.js \
    "$OUTPUT_DIR/old-pipeline.pretty.js" \
    "$OUTPUT_DIR/new-pipeline.pretty.js" \
    "$OUTPUT_DIR/old-vs-new.pretty.diff"

echo
echo "Generated:"
echo "  $OUTPUT_DIR/vue-vs-new.pretty.diff"
echo "  $OUTPUT_DIR/old-vs-new.pretty.diff"
echo
echo "Open with:"
echo "  bat --language=diff --paging=always $OUTPUT_DIR/vue-vs-new.pretty.diff"
