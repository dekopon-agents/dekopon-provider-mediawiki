# Provider authoring experience

This document extracts provider-design lessons from the MediaWiki implementation and concrete ingredients for a future **design your Dekopon provider** skill. The chronological command/failure record is in [`../AUTHORING.md`](../AUTHORING.md).

## Decisions that mattered

- **Start from authority, not an API client.** Five read-only capabilities form a guided path: search → compact lead → outline → one section → links. There is no generic URL/API/wikitext escape hatch and no sixth convenience tool.
- **Use the current component contract.** The provider includes `dekopon:provider/provider@0.2.0`, imports only `dekopon:http/client@1.0.0`, and uses `export_provider_with_bindings!`. HTTP providers must be exercised through `dekopon-brokerd`; the import-free direct runner is intentionally the wrong host.
- **Treat schemas as prompt metadata, not enforcement.** Every request is separately deserialized with `deny_unknown_fields`, then checked for semantic, scalar-count, UTF-8-byte, and allowlist bounds before the first host call.
- **Make destination choice non-input.** A reviewed SiteMatrix snapshot supplies language labels. The implementation constructs HTTPS/443 Wikipedia Action API origins and accepts no URL, host, scheme, port, project, IP, credential, environment variable, or runtime configuration.
- **Pin page identity and namespace before parsing.** `wikipedia_outline` first resolves a main-namespace page and revision with `action=query`, then parses sections by `oldid`. `wikipedia_section` adds one final `oldid`/index fetch. The two- and three-call sequences prevent namespace bypass and revision races.
- **Budget serialized data, not characters alone.** Four-byte Unicode and JSON escaping make character limits insufficient. Field caps feed a 14,000-byte projected-output check and an actual 16,384-byte SDK-envelope check.

## SDK, WIT, and HTTP lessons

1. `ProviderManifest` in SDK `0.10.0` still contains `command_words`; base-only providers set it to an empty vector and export with `export_provider_with_bindings!`, not the command macro.
2. A caller-generated WIT world is the composition point for privileged imports. Including the provider world does not grant HTTP; the broker links it only after authorization.
3. `dekopon-provider-http` is a buffered facade. The guest must independently cap bodies before JSON/DOM parsing even when broker constraints also cap them.
4. `HttpErrorCode::Timeout` and `ResponseTooLarge` are useful stable classes. All other transport detail should collapse into a sanitized provider error rather than become model-visible diagnostics.
5. The broker does not follow redirects or inherit proxy settings. Provider code should still reject every 3xx and never construct a request from returned content.
6. Mirrored WIT should be compared with the exact crate versions resolved in `Cargo.lock`; matching a branch is weaker than matching the bytes that were compiled.

## API and tool-design lessons

- MediaWiki search snippets are HTML fragments even when the desired product is plain text. Browser-grade parsing is necessary for entity decoding and malformed markup; regex stripping is not a safe fallback.
- Action API continuation is tool-specific. A cursor must retain both generic `continue` and `sroffset`/`srcontinue` or `plcontinue`; silently dropping one can restart or corrupt pagination.
- Cursor opacity is not authentication. A no-secret, no-clock ABI can bind cursor shape and request fingerprint, but documentation must not claim tamper resistance or expiry.
- Redirects, normalization, conversion, disambiguation, and missing pages are distinct states. Compact tools should preserve requested and canonical identity and flag disambiguation instead of guessing.
- An outline index is safer than accepting headings. Headings are mutable and non-unique; the provider asks callers to copy the API-issued index from a fresh outline.
- Section HTML can include edit controls, nested headings, citations, images, navigation boxes, and tables. Projection rules need adversarial fixtures and must never dereference embedded resources.

## Testing and shipping setup

Native tests inject a scripted `FnMut(Request)` and assert call order, fixed authority, form encoding, constant headers, absent authorization, response projection, pagination, and failures. Fixtures cover empty/paginated search, redirect/disambiguation/missing pages, outline and revision-pinned sections, links continuation, and hostile HTML. Live network checks are intentionally broker-level manual smoke tests.

The repeatable local command sequence is:

```console
cargo +1.89.0 check --locked --all-targets --package dekopon-mediawiki-provider
cargo fmt --all -- --check
cargo test --locked --package dekopon-mediawiki-provider
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

`scripts/validate.sh` is the shared local/CI/release gate. CI adds independent-checkout reproducibility, dependency bans, mirrored-WIT equality, component import/export inspection, path scans, and artifact upload. Release accepts strict stable semantic-version tags only, requires an annotated tag matching `Cargo.toml` and contained in `main`, creates or strictly reuses a draft with exactly two assets, publishes the same Wasm bytes to GHCR, and finalizes only after all prior steps succeed. Build, attestation, draft, GHCR, and finalization are separate artifact-linked jobs with only their required permissions. These are implemented checks, not a claim that a release has run.

## Bounded review and repair — 2026-08-22

The Rust/security and product/release reviews found no fifth/sixth-tool or secret-surface problem, but identified one high implementation risk, one high workflow risk, and several bounded correctness gaps. Every reported code/workflow item was repaired before repository creation:

- Replaced recursive DOM walking with iterative enter/exit frames. A complete preflight traversal rejects more than 50,000 nodes or depth above 256 as `response_too_large`, so hostile markup cannot turn guest stack exhaustion into a Wasm trap.
- Added bounded formula projection: prefer `application/x-tex` annotation text, then MediaWiki math fallback `alt`; never fetch `src`. Missing formula text emits `[formula omitted]`, and omission/shortening propagates `truncated` where supported.
- Closed the outline/section namespace bypass. Both resolve with `action=query&prop=info`, require `ns == 0`, and pin `lastrevid`; outline then parses that `oldid`, and section fetches the chosen index from the same revision. Broker request budgets became two for outline and three for section. Missing/deleted pages now remain `not_found`, not `no_such_section`.
- Changed depth-cap pagination from ambiguous exhaustion to explicit `pagination_capped`. A null cursor plus `true` means upstream had more data but depth ten stopped traversal.
- Lowered links from 50/default 25 to a worst-case-safe maximum/default 20. A unit test uses twenty distinct 255-byte maximally escaped titles, a 255-byte maximally escaped page title, and a full 2 KiB cursor: projected JSON is exactly 13,074 bytes and the SDK envelope 13,107, below their 14,000/16,384-byte ceilings.
- Split the tag workflow into read-only build, attestation-only, contents-write draft, packages-write GHCR, and contents-write finalization jobs. Artifact downloads are checksum-verified, action references use commit SHAs (including peeled attestation commit `e8998f…`), and every `wasm-tools` install runs under Rust 1.97 explicitly.
- Made release reruns recoverable: a matching existing draft is downloaded and byte-compared before reuse; only a draft newly created by the failing draft job is cleaned up. A later GHCR failure intentionally leaves a verified reusable draft.
- Expanded README examples with response fragments, `sections[].index` mapping, and second-page calls that retain the original parameters.

The cursor remains forgeable/replayable by the already documented no-key/no-clock design; depth is a context bound, not an authorization boundary. The repaired tree passed 40 tests, native/Wasm Clippy, MSRV and release checks, `actionlint`, `shellcheck`, and `zizmor` (no findings). Two independent ordinary-target builds produced 817,527-byte components with SHA-256 `3725b550e93acf1d0b72e9638c00033c51d276c016fafbb99014877eefb7d8d6`. End-to-end broker smoke and real GitHub release evidence remained pending at this repair checkpoint and are recorded below only after observation.

## Friction and fixes

- The sister provider was on SDK `0.9.0`; reading the published `0.10.0` crates and tagged core source avoided copying a stale dependency/command precedent.
- The proven build harness used `CARGO_TARGET_DIR` to obtain a second target. Machine-wide worktree rules prohibit that. The replacement uses independent source checkouts, each with its own ordinary default `target/`, while preserving the deterministic rustc proxy and remapping.
- Character-oriented API limits do not guarantee a bounded JSON envelope. Output types with `truncated` shrink text or tail sections after measuring serialization; types without that field use tighter per-field caps or fail `response_too_large` rather than lie about continuation.
- MediaWiki's parse-section response repeats the selected heading and may include subsections. The projector removes only a matching first heading line, preserves nested headings, and still bounds the complete selected section.

## Ingredients for a future skill

A future design skill should request and produce:

1. **Authority worksheet:** provider identity, exact capability cap, effect/risk/idempotency, forbidden generic surfaces, imports, and broker-only execution assumptions.
2. **Trust-boundary worksheet:** every caller-controlled field, every derived destination/path/header, credentials, redirects, retries, clocks, storage, and ambient dependencies.
3. **Budget table:** input scalar/byte bounds, host-call count, request/response bytes, collection caps, cursor depth/bytes, projected output, final envelope, memory/fuel/timeout.
4. **API-state table:** success, empty, missing, normalization, redirect, ambiguity, pagination, stale selection, rate limit, overload, timeout, malformed and oversized responses.
5. **Fixture plan:** exact request assertions, hostile structured content, four-byte Unicode, JSON escaping, and no-call validation tests.
6. **WIT/build checklist:** crate/WIT/toolchain pins, mirror equality, only expected imports/exports, banned ambient crates, path remapping, independent reproducibility.
7. **Broker deployment template:** exact hosts and methods per capability, call counts (including pre-reads), 10-second timeout, 1 MiB response ceiling, and 16 KiB output ceiling.
8. **Release transaction:** strict annotated tag → shared gates → reproducibility → attestation → draft/exact assets → identical OCI layer → finalize → post-release byte verification.
9. **Evidence discipline:** separate planned checks from observed outcomes and reserve explicit fields for hashes, workflow URLs, OCI digests, anonymous pull results, and broker smoke transcripts.
