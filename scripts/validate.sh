#!/usr/bin/env bash
# Shared local, CI, and release shipping gates.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"

command -v jq >/dev/null 2>&1 || {
  echo "error: jq is required" >&2
  exit 1
}
command -v wasm-tools >/dev/null 2>&1 || {
  echo "error: wasm-tools 1.259.0 is required" >&2
  exit 1
}
if [[ "$(wasm-tools --version)" != "wasm-tools 1.259.0" ]]; then
  echo "error: expected wasm-tools 1.259.0" >&2
  exit 1
fi

if ! rustup run 1.98.1 rustc --version >/dev/null 2>&1; then
  echo "error: Rust 1.98.1 is required for the declared MSRV check" >&2
  exit 1
fi
cargo +1.98.1 check --locked --all-targets --package dekopon-mediawiki-provider
cargo fmt --all -- --check
cargo test --locked --package dekopon-mediawiki-provider
cargo clippy --all-targets --locked --package dekopon-mediawiki-provider -- -D warnings
cargo check --locked --package dekopon-mediawiki-provider --target wasm32-unknown-unknown
cargo clippy --locked --package dekopon-mediawiki-provider --target wasm32-unknown-unknown --lib -- -D warnings

metadata=$(cargo metadata --locked --format-version 1)
sdk_manifest=$(jq -er '.packages[] | select(.name == "dekopon-provider-sdk" and .version == "0.13.0") | .manifest_path' <<<"$metadata")
http_manifest=$(jq -er '.packages[] | select(.name == "dekopon-provider-http" and .version == "0.13.0") | .manifest_path' <<<"$metadata")
wit_version=$(jq -er '.packages[] | select(.name == "wit-bindgen" and .version == "0.62.0") | .version' <<<"$metadata")
[[ "$wit_version" == "0.62.0" ]]
cmp "$(dirname "$sdk_manifest")/wit/provider.wit" wit/deps/provider.wit
cmp "$(dirname "$http_manifest")/wit/deps/http.wit" wit/deps/http.wit

mkdir -p target/validation
cargo tree --locked --target wasm32-unknown-unknown --edges normal,build \
  --prefix none --format '{p}' | sort -u >target/validation/deps.tree
if grep -Eqi '^(wasi([^[:alnum:]]|$)|wasm-bindgen([^[:alnum:]]|$)|js-sys([^[:alnum:]]|$))' target/validation/deps.tree; then
  echo "error: forbidden ambient dependency" >&2
  grep -Ein '^(wasi([^[:alnum:]]|$)|wasm-bindgen([^[:alnum:]]|$)|js-sys([^[:alnum:]]|$))' target/validation/deps.tree >&2
  exit 1
fi
if rg -n '\bunsafe\b' src; then
  echo "error: handwritten unsafe source is forbidden" >&2
  exit 1
fi

./build.sh
component=mediawiki-provider.wasm
checksum=mediawiki-provider.wasm.sha256
core=target/wasm32-unknown-unknown/release/dekopon_mediawiki_provider.wasm
test -s "$component"
test -s "$checksum"
test -s "$core"
wasm-tools validate "$core"
wasm-tools validate "$component"
wasm-tools metadata show "$core" >target/validation/core-metadata.txt
wasm-tools metadata show "$component" >target/validation/component-metadata.txt
grep -F 'wit-bindgen-rust' target/validation/component-metadata.txt >/dev/null

wasm-tools print "$core" | grep '(import ' >target/validation/core-imports.txt || true
test -s target/validation/core-imports.txt
if grep -Fv 'dekopon:http/client@1.0.0' target/validation/core-imports.txt; then
  echo "error: unexpected core import" >&2
  exit 1
fi
if [[ $(wc -l <target/validation/core-imports.txt | tr -d ' ') != 1 ]]; then
  echo "error: expected exactly one core import" >&2
  exit 1
fi

wasm-tools component wit "$component" >target/validation/component.wit
wasm-tools component wit -j "$component" >target/validation/component-wit.json
jq -e '
  (.worlds | length) == 1 and
  (.worlds[0].imports | length) == 1 and
  ((.worlds[0].exports | keys | sort) == ["describe", "invoke", "run-command"]) and
  (.interfaces | length) == 1 and
  (.interfaces[0].name == "client") and
  ((.interfaces[0].functions | keys) == ["send"]) and
  (.packages[.interfaces[0].package].name == "dekopon:http@1.0.0")
' target/validation/component-wit.json >/dev/null
if grep -Eq 'wasi:|resolve-command' target/validation/component.wit; then
  echo "error: component exposes an ambient import or extra export" >&2
  exit 1
fi

expected=$(awk '{print $1}' "$checksum")
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$component" | awk '{print $1}')
else
  actual=$(shasum -a 256 "$component" | awk '{print $1}')
fi
[[ "$actual" == "$expected" ]]
[[ $(awk 'NF {count++} END {print count+0}' "$checksum") == 1 ]]
[[ $(awk '{print $2}' "$checksum") == "mediawiki-provider.wasm" ]]

for forbidden in "$root" "${CARGO_HOME:-$HOME/.cargo}" "$(rustc --print sysroot)"; do
  if LC_ALL=C grep -aF -- "$forbidden" "$component" >/dev/null; then
    echo "error: component embeds local path $forbidden" >&2
    exit 1
  fi
done

printf 'all provider shipping gates passed; sha256=%s\n' "$actual"
