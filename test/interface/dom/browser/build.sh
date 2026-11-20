#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../../.."
case "${1:-dom}" in
    dom)
        package=blog-dom
        example=dom-contracts
        fixture=dom-fixture
        ;;
    corner-listener)
        package=blog-interface
        example=corner-listener-contracts
        fixture=corner-listener-fixture
        ;;
    *) echo "Unknown browser fixture: $1" >&2; exit 2 ;;
esac
fixture_output="$PWD/target/verification/anti-defensive-review/$fixture"
mkdir -p "$fixture_output/pkg"
CARGO_TARGET_DIR="${BLOG_WASM_TARGET_DIR:-${CARGO_TARGET_DIR:-$PWD/target}}" \
    cargo build --locked --offline -p "$package" --example "$example" \
    --target wasm32-unknown-unknown --profile web-release --message-format=json > "$fixture_output/wasm-build.jsonl"
wasm=$(python3 - "$fixture_output/wasm-build.jsonl" "$example" <<'PY'
import json
from pathlib import Path
import sys

log, example = sys.argv[1:]
matches = [
    filename
    for line in Path(log).read_text().splitlines()
    if (item := json.loads(line)).get('reason') == 'compiler-artifact'
    and item['target']['name'] == example
    and 'example' in item['target']['kind']
    for filename in item['filenames']
    if filename.endswith('.wasm')
]
if len(matches) != 1:
    raise SystemExit(f'Expected one WASM artifact for {example}, got {matches}')
print(matches[0])
PY
)
cargo run --locked --offline -p blog-assets --example browser-fixture-bindings -- "$wasm" "$fixture_output/pkg"
printf '%s\n' "$fixture_output/pkg"
