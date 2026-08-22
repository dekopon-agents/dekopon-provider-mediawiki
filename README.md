# dekopon-provider-mediawiki

A standalone WebAssembly component providing five bounded, read-only Wikipedia capabilities to [Dekopon](https://github.com/dekopon-agents/dekopon).

The tool path is intentionally small and sequential:

1. `wikipedia_search` — find candidate titles;
2. `wikipedia_page` — read a compact canonical lead and identity;
3. `wikipedia_outline` — choose a section index;
4. `wikipedia_section` — retrieve exactly that revision-pinned section;
5. `wikipedia_links` — paginate main-namespace links for controlled follow-up.

There is no generic API/URL tool, whole-article dump, HTML, wikitext, references, categories, backlinks, images, random-page operation, Wikidata expansion, write operation, or sixth capability.

## Context safety

Inputs and outputs are bounded in native code, not only JSON Schema. Search returns at most 10 results; leads at most 1,200 characters; outlines at most 60 entries; one section at most 8,000 characters; and links at most 50 entries from one API page. Snippets and HTML-derived text are DOM-decoded plaintext. Upstream bodies stop at 1 MiB, projected JSON stays below 14,000 bytes, and the measured SDK envelope stays at or below 16,384 bytes, including four-byte Unicode and JSON escaping.

Pagination performs one API page per invocation and never drains continuation automatically. Opaque cursors are request-bound, at most 2 KiB, and stop after ten continuation depths. They are deliberately **not** claimed to be authenticated or expiring: the current guest ABI supplies no host key or clock.

The component makes zero automatic retries. It returns compact actionable errors such as `invalid_language`, `invalid_cursor`, `not_found`, `no_such_section`, `rate_limited`, `maxlag`, `timeout`, and `response_too_large` without response bodies or transport details.

## Example calls

From a Dekopon shell session, follow the guided surface rather than requesting a whole article:

```sh
cap wikipedia_search '{"query":"Ada Lovelace","language":"en","limit":3}'
cap wikipedia_page '{"title":"Ada Lovelace","language":"en","max_chars":900}'
cap wikipedia_outline '{"title":"Ada Lovelace","language":"en","max_sections":20}'
cap wikipedia_section '{"title":"Ada Lovelace","section_index":"1","language":"en","max_chars":3000}'
cap wikipedia_links '{"title":"Ada Lovelace","language":"en","limit":25}'
```

Copy `next_cursor` unchanged into the same search or links request to fetch one more page. Copy `section_index` only from a fresh outline. `language` defaults to `en` and is checked against a dated, checked-in list of active Wikipedia editions.

## Install and broker configuration

The component grants nothing by itself. Install `mediawiki-provider.wasm` in an owner-controlled provider directory loaded by `dekopon-brokerd`. Once releases exist, the same bytes will be available as the release asset and as an OCI layer at `ghcr.io/dekopon-agents/provider-mediawiki:<version>`, with `mediawiki-provider.wasm.sha256` alongside the release asset.

Broker authority must list each enabled language host exactly—never `*.wikipedia.org`—and permit HTTPS/443 GET only. A representative English-only fragment is:

```yaml
providers:
  - /opt/dekopon/providers/mediawiki-provider.wasm

constraintSets:
  wikipedia_search:
    provider: mediawiki
    effect: read-only
    risk: Low
    idempotency: idempotent
    constraints: &wikipediaOneRequest
      timeoutMs: 10000
      maxOutputBytes: 16384
      http:
        allowedHosts: [en.wikipedia.org]
        allowedMethods: [GET]
        maxRequests: 1
        maxRequestBytes: 16384
        maxResponseBytes: 1048576
        allowPlaintextLoopback: false
  wikipedia_page:
    provider: mediawiki
    effect: read-only
    risk: Low
    idempotency: idempotent
    constraints: *wikipediaOneRequest
  wikipedia_outline:
    provider: mediawiki
    effect: read-only
    risk: Low
    idempotency: idempotent
    constraints: *wikipediaOneRequest
  wikipedia_links:
    provider: mediawiki
    effect: read-only
    risk: Low
    idempotency: idempotent
    constraints: *wikipediaOneRequest
  wikipedia_section:
    provider: mediawiki
    effect: read-only
    risk: Low
    idempotency: idempotent
    constraints:
      timeoutMs: 10000
      maxOutputBytes: 16384
      http:
        allowedHosts: [en.wikipedia.org]
        allowedMethods: [GET]
        maxRequests: 2
        maxRequestBytes: 16384
        maxResponseBytes: 1048576
        allowPlaintextLoopback: false
```

Add `de.wikipedia.org` (or another checked-in edition) explicitly to every capability that may use it. Do not configure credentials: the guest never sets `authorization`, and Wikipedia reads are public. Add ordinary deny-by-default Cedar permits for only the principals and capability actions that should use these tools.

HTTP imports are linked only by the broker. The import-free direct `dekopon-run` execution path is expected to reject this component.

## Build and validate

The release toolchain and encoder are exact pins:

```console
rustup toolchain install 1.89.0 --profile minimal
rustup toolchain install 1.97.0 --profile minimal --component clippy --component rustfmt
rustup target add wasm32-unknown-unknown --toolchain 1.97.0
cargo install wasm-tools --version 1.236.1 --locked
cargo test --locked --package dekopon-mediawiki-provider
./scripts/validate.sh
./build.sh
```

`build.sh` preserves the proven Dekopon deterministic metadata normalization, source/Cargo/sysroot path remapping, and path scan. It uses the checkout's ordinary `target/` and never redirects Cargo to a shared build directory. CI proves reproducibility from two independent checkouts with separate default targets.

## Manual broker smoke checklist

After installing the built component under a constrained `dekopon-brokerd`, verify English and German search; `Ada Lovelace`; redirects through `NYC`; disambiguation through `Mercury`; a deliberately missing title; outline → copied section index; and two links pages using the returned cursor. Record actual evidence in [`AUTHORING.md`](AUTHORING.md); none is claimed before it is run.

## Design notes

Implementation and reusable provider-design lessons are in [`docs/provider-authoring-experience.md`](docs/provider-authoring-experience.md). The from-scratch chronological record is [`AUTHORING.md`](AUTHORING.md).

## License

MIT or Apache-2.0, at your option.
