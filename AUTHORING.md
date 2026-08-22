# Authoring log

This is the chronological, from-scratch build record for the MediaWiki provider. It records only work that has happened; release results will be appended after a release exists.

## 2026-08-22 — contract and source selection

1. Confirmed the target directory did not exist, the sister `dekopon-provider-gh` checkout was clean at `v0.1.0`, and the Dekopon core checkout was clean. The core checkout was not modified.
2. Read the out-of-tree GitHub provider, Dekopon's HTTP provider examples, and the published `dekopon-provider-sdk` / `dekopon-provider-http` `0.10.0` sources.
3. Chose the base `dekopon:provider/provider@0.2.0` world with only `describe` and `invoke`, plus the sole `dekopon:http/client@1.0.0` import. No command word or resolver is exported.
4. Kept the approved five-capability ceiling: search, compact page lead, outline, one revision-pinned section, and paginated main-namespace links.
5. Queried the public SiteMatrix API solely during authoring and checked in the 348 active, non-closed Wikipedia language host labels returned on 2026-08-22. Invocation never performs host discovery.

Commands used for the contract preflight included:

```console
cargo info dekopon-provider-sdk@0.10.0
cargo info dekopon-provider-http@0.10.0
wasm-tools --version
curl 'https://meta.wikimedia.org/w/api.php?action=sitematrix&format=json&formatversion=2&smtype=language&smsiteprop=code%7Curl%7Cclosed%7Cprivate%7Cfishbowl'
```

## 2026-08-22 — implementation plan

- Strict `serde(deny_unknown_fields)` request types repeat every model-facing schema bound in native code.
- The guest constructs only `https://<allowlisted-language>.wikipedia.org/w/api.php` GET requests with a form encoder, constant headers, no credentials, and no endpoint override.
- Search and links consume one upstream page per invocation. Their cursors are versioned base64url envelopes containing only allowlisted continuation fields, a request fingerprint, tool kind, and bounded depth. They are intentionally not described as authenticated or expiring because the ABI provides no host key or clock.
- Section reads first resolve the requested index and revision with `action=parse`, then fetch that exact section by `oldid` and index. This costs exactly two sequential calls and prevents a revision race.
- HTML is parsed into a DOM; executable/resource/navigation/edit/reference nodes are skipped and text is projected with block boundaries. No content URL is fetched.
- Upstream bodies stop at 1 MiB, projected outputs stop below 14,000 bytes, and the complete SDK success envelope is measured against 16,384 bytes.
- The deterministic component build retains the proven metadata normalization and path remapping, but never sets `CARGO_TARGET_DIR`; independent checkouts provide independent default Cargo targets for reproducibility checks.

## 2026-08-22 — implementation, friction, and fixes

1. Scaffolded a standalone Rust 2024 `cdylib` with exact SDK/HTTP `0.10.0` and `wit-bindgen 0.44.0` pins, mirrored WIT, dual licensing, a default-target deterministic build harness, and local Git metadata with no remote.
2. Implemented strict inputs, the dated language allowlist, fixed form-encoded requests, typed permissive-upstream projections, versioned request-bound cursors, DOM plaintext extraction, revision-pinned sections, stable sanitized errors, and exact serialized-output/envelope budgets.
3. Added fixture-driven native tests for all five tools, boundaries, pagination, normalization/redirects, missing/disambiguation behavior, hostile markup, request order, transport/status/API errors, 1 MiB + 1 responses, and four-byte/escaped output.
4. Added shared shipping gates plus pinned CI/release workflows. Release remains a transaction: strict annotated stable SemVer tag in `main`, gates and independent rebuild, Wasm attestation, draft with exactly two assets, identical Wasm layer to GHCR, then finalization.

Observed friction and fixes:

- The first generic input decoder calls did not infer their concrete request types on Rust 1.97. Explicit local types made the second validation layer unambiguous.
- `clippy -D warnings` found an explicit counter loop in UTF-8 truncation. Replacing it with `char_indices().enumerate()` retained scalar/byte semantics without suppressing the lint.
- Repeating all 348 languages as a JSON Schema enum in each capability produced a 42 KiB manifest. The model-facing schema now gives a compact lexical pattern and examples; the complete 348-entry allowlist remains native and tested. The checked snapshot is 7,145 bytes.
- The first validation script assumed the stripped core module retained `wit-bindgen-rust` producer metadata. The encoded component does retain it, while the authoritative core evidence is its exact single import. The gate now checks component producer metadata and independently inspects core imports.
- The proven sister harness redirected a second build with `CARGO_TARGET_DIR`, which is prohibited on this machine. Reproducibility instead used an independent source copy with its own ordinary default `target/`; the temporary copy was removed after byte comparison, without `cargo clean`.

## Validation record

The final shared gate ran successfully:

```console
cargo +1.89.0 check --locked --all-targets --package dekopon-mediawiki-provider
cargo fmt --all -- --check
cargo test --locked --package dekopon-mediawiki-provider        # 34 passed
cargo clippy --all-targets --locked --package dekopon-mediawiki-provider -- -D warnings
cargo check --locked --package dekopon-mediawiki-provider --target wasm32-unknown-unknown
cargo clippy --locked --package dekopon-mediawiki-provider --target wasm32-unknown-unknown --lib -- -D warnings
./build.sh
wasm-tools validate mediawiki-provider.wasm
wasm-tools component wit mediawiki-provider.wasm
actionlint -no-color
shellcheck build.sh scripts/validate.sh
./scripts/validate.sh
```

Observed component evidence:

- artifact size: `800468` bytes;
- SHA-256: `dc1a5c864681665acd350d59d1c677f5e32f135be5d8a505c02e9fc99a7c3ea2`;
- sole core import: `dekopon:http/client@1.0.0` function `send`;
- component exports: exactly `describe` and `invoke`;
- no WASI import, command resolver, banned `wasi` / `wasm-bindgen` / `js-sys` dependency, handwritten `unsafe`, or embedded source/Cargo/sysroot path;
- mirrored SDK and HTTP WIT matched the exact `0.10.0` crates resolved by `Cargo.lock`.

A second source copy, excluding `.git`, `target/`, and generated artifacts, ran `./build.sh` with its own default Cargo target. `cmp` matched both the Wasm and checksum byte-for-byte at the hash above. The temporary rebuild tree was then removed.

No broker smoke was run in this phase: HTTP execution requires an operator-configured `dekopon-brokerd`, and creating/deploying that configuration was outside the no-remote/no-publish build task. The exact English/German, `ada_lovelace`, `NYC`, `Mercury`, missing-title, outline→section, and two-page links checklist remains explicit in `README.md` for post-review execution.

No GitHub repository, remote, pull request, release, tag, package, attestation, or OCI artifact has been created by this build phase.

## Release record

Not released. Do not fill this section from planned workflow behavior; append observed tag, workflow, attestation, release checksum, and OCI digest only after publication.

## Reusable checklist

1. Freeze the capability cap, effects, risks, idempotency, input schemas, output projections, and stable errors before coding.
2. Pin the Rust crates and mirror their WIT byte-for-byte; compose a provider-owned world with only reviewed imports.
3. Validate untrusted JSON twice: closed schema for the model and strict native deserialization plus semantic/byte bounds.
4. Derive every destination from checked-in data; never accept an origin, URL, credential, or ambient configuration.
5. Inject HTTP behind `FnMut(Request) -> Result<Response, HttpError>` and test exact requests without network access.
6. Bound request count, body bytes, pagination depth, parsed fields, text scalars, UTF-8 bytes, projected JSON, and final SDK envelope independently.
7. Use a real parser for structured upstream content and test malformed/adversarial fixtures.
8. Pin mutable build tools, remap local paths, inspect imports/exports, and compare independent builds.
9. Make release publication fail closed: annotated/version-matched/main-contained tag, draft first, provenance, exact assets, identical OCI bytes, finalize last.
10. Record actual failures, fixes, hashes, and broker smoke evidence; never backfill planned outcomes as facts.
