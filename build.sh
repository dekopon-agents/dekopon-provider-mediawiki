#!/usr/bin/env bash
# Build mediawiki-provider.wasm reproducibly with ordinary per-checkout Cargo targets.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
manifest="$root/Cargo.toml"
target_root="$root/target"
core="$target_root/wasm32-unknown-unknown/release/dekopon_mediawiki_provider.wasm"
component=${1:-"$root/mediawiki-provider.wasm"}

rust_toolchain="1.98.1"
required_rustc="rustc 1.98.1 (48a229cea 2026-09-01)"
required_wasm_tools_version="1.259.0"
metadata_domain="dekopon-provider-repro-v1"

if [[ -n "${CARGO_TARGET_DIR-}" ]]; then
  echo "error: use this checkout's ordinary target/; CARGO_TARGET_DIR must not be set" >&2
  exit 1
fi
command -v rustup >/dev/null 2>&1 || {
  echo "error: rustup with Rust $rust_toolchain is required" >&2
  exit 1
}
actual_rustc=$(rustup run "$rust_toolchain" rustc --version 2>/dev/null) || {
  echo "error: install Rust $rust_toolchain with rustup" >&2
  exit 1
}
if [[ "$actual_rustc" != "$required_rustc" ]]; then
  echo "error: expected $required_rustc, found $actual_rustc" >&2
  exit 1
fi
command -v wasm-tools >/dev/null 2>&1 || {
  echo "error: wasm-tools $required_wasm_tools_version is required" >&2
  exit 1
}
actual_wasm_tools=$(wasm-tools --version)
actual_wasm_tools_version=${actual_wasm_tools#wasm-tools }
actual_wasm_tools_version=${actual_wasm_tools_version%% *}
if [[ "$actual_wasm_tools_version" != "$required_wasm_tools_version" ]]; then
  echo "error: expected wasm-tools $required_wasm_tools_version, found $actual_wasm_tools" >&2
  exit 1
fi

mkdir -p "$target_root" "$(dirname "$component")"
cargo_home=${CARGO_HOME:-"$HOME/.cargo"}
cargo_home=$(cd "$cargo_home" && pwd -P)
sysroot=$(rustup run "$rust_toolchain" rustc --print sysroot)
sysroot=$(cd "$sysroot" && pwd -P)
rustc_path=$(rustup which --toolchain "$rust_toolchain" rustc)
rustc_proxy="$target_root/deterministic-rustc"
cat >"$rustc_proxy" <<'PROXY'
#!/usr/bin/env bash
set -euo pipefail

actual_rustc=${DEKOPON_BUILD_RUSTC:?}
source_root=${DEKOPON_BUILD_SOURCE_ROOT:?}
metadata_domain=${DEKOPON_BUILD_METADATA_DOMAIN:?}
manifest_dir=${CARGO_MANIFEST_DIR-}
repository_crate=false
if [[ "$manifest_dir" == "$source_root" || "$manifest_dir" == "$source_root/"* ]]; then
  repository_crate=true
fi

target=host
expect_target=false
for argument in "$@"; do
  if [[ "$expect_target" == true ]]; then
    target=$argument
    expect_target=false
    continue
  fi
  case $argument in
    --target) expect_target=true ;;
    --target=*) target=${argument#--target=} ;;
  esac
done

normalize_metadata=$repository_crate
if [[ "$target" == wasm32-unknown-unknown ]]; then
  normalize_metadata=true
fi

args=()
crate_name=
while (($#)); do
  case $1 in
    --crate-name)
      crate_name=$2
      args+=("$1" "$2")
      shift 2
      ;;
    --target)
      target=$2
      args+=("$1" "$2")
      shift 2
      ;;
    --target=*)
      target=${1#--target=}
      args+=("$1")
      shift
      ;;
    -C)
      if (($# >= 2)) && [[ $2 == metadata=* ]] && [[ "$normalize_metadata" == true ]]; then
        shift 2
      else
        args+=("$1")
        shift
      fi
      ;;
    -Cmetadata=*)
      if [[ "$normalize_metadata" == true ]]; then
        shift
      else
        args+=("$1")
        shift
      fi
      ;;
    *)
      args+=("$1")
      shift
      ;;
  esac
done

if [[ "$normalize_metadata" == true && -n "$crate_name" && -n "${CARGO_PKG_NAME-}" && -n "${CARGO_PKG_VERSION-}" ]]; then
  args+=(
    -C
    "metadata=$metadata_domain-${CARGO_PKG_NAME}-${CARGO_PKG_VERSION}-$crate_name-$target"
  )
fi
exec "$actual_rustc" "${args[@]}"
PROXY
chmod 0700 "$rustc_proxy"

rustflags=(
  "--remap-path-prefix=$root=/dekopon/source"
  "--remap-path-prefix=$cargo_home=/dekopon/cargo"
  "--remap-path-prefix=$sysroot=/dekopon/rust/$rust_toolchain"
  '--cfg=dekopon_provider_repro_v1'
  '--check-cfg=cfg(dekopon_provider_repro_v1)'
  '-Ccodegen-units=1'
)
encoded_rustflags=$(printf '%s\x1f' "${rustflags[@]}")
encoded_rustflags=${encoded_rustflags%$'\x1f'}

rustup target add --toolchain "$rust_toolchain" wasm32-unknown-unknown
CARGO_ENCODED_RUSTFLAGS="$encoded_rustflags" \
  DEKOPON_BUILD_RUSTC="$rustc_path" \
  DEKOPON_BUILD_SOURCE_ROOT="$root" \
  DEKOPON_BUILD_METADATA_DOMAIN="$metadata_domain" \
  RUSTC="$rustc_proxy" \
  rustup run "$rust_toolchain" cargo build \
    --locked --manifest-path "$manifest" --package dekopon-mediawiki-provider \
    --target wasm32-unknown-unknown --release

wasm-tools component new "$core" -o "$component"
wasm-tools validate "$component"
for local_path in "$root" "$cargo_home" "$sysroot"; do
  if LC_ALL=C grep -aF -- "$local_path" "$component" >/dev/null; then
    echo "error: generated component embeds local build path: $local_path" >&2
    exit 1
  fi
done

if command -v sha256sum >/dev/null 2>&1; then
  hash=$(sha256sum "$component" | awk '{print $1}')
else
  hash=$(shasum -a 256 "$component" | awk '{print $1}')
fi
(
  cd "$(dirname "$component")"
  printf '%s  %s\n' "$hash" "$(basename "$component")" >"$(basename "$component").sha256"
)
printf 'generated %s (%s) with Rust %s\n' "$component" "$hash" "$rust_toolchain"
