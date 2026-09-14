# dekopon-provider-mediawiki

A standalone WebAssembly component giving [Dekopon](https://github.com/dekopon-agents/dekopon) five bounded, read-only Wikipedia capabilities behind one command word, `wikipedia`.

The path is intentionally small and sequential:

1. `wikipedia search` (`wikipedia_search`) — find candidate titles;
2. `wikipedia page` (`wikipedia_page`) — read a compact canonical lead and identity;
3. `wikipedia outline` (`wikipedia_outline`) — choose a section index;
4. `wikipedia section` (`wikipedia_section`) — retrieve exactly that revision-pinned section;
5. `wikipedia links` (`wikipedia_links`) — paginate main-namespace links for controlled follow-up.

There is no generic API/URL tool, whole-article dump, HTML, wikitext, references, categories, backlinks, images, random-page operation, Wikidata expansion, write operation, or sixth capability.

## Context safety

Inputs and outputs are bounded in native code, not only JSON Schema. Search returns at most 10 results; leads at most 1,200 characters; outlines at most 60 entries; one section at most 8,000 characters; and links at most 20 entries from one API page. The 20-link cap is proven against 255-byte fully JSON-escaped titles plus a full 2 KiB cursor. Snippets and HTML-derived text are DOM-decoded plaintext with iterative node/depth limits. Math is projected from bounded TeX or formula alt text without fetching resources; an unavailable formula becomes `[formula omitted]` and marks truncation where the output supports it. Upstream bodies stop at 1 MiB, projected JSON stays below 14,000 bytes, and the measured SDK envelope stays at or below 16,384 bytes, including four-byte Unicode and JSON escaping.

Pagination performs one API page per invocation and never drains continuation automatically. Opaque cursors are request-bound, at most 2 KiB, and stop after ten continuation depths. `pagination_capped: true` with `next_cursor: null` means Wikipedia still advertised another page but the provider depth cap stopped traversal; `false` with a null cursor means natural exhaustion. Cursors are deliberately **not** claimed to be authenticated or expiring: the current guest ABI supplies no host key or clock.

The component makes zero automatic retries. It returns compact actionable errors such as `invalid_language`, `invalid_cursor`, `not_found`, `no_such_section`, `rate_limited`, `maxlag`, `timeout`, and `response_too_large` without response bodies or transport details.

## The `wikipedia` command word

The component parses its own argv with clap. `--help`, `--version`, and every usage error render inside the guest and authorize nothing. A well-formed argv becomes a proposal for one capability, spelled as that capability's input: each flag is the kebab-case form of one snake_case field (`--max-chars 1200` proposes `"max_chars": 1200`), every field is sent with its default filled in, and the proposal takes the ordinary path — constraint set, Cedar, broker HTTP. An integer outside its ceiling is a usage error naming the flag; everything else is checked in `invoke`.

```console
$ wikipedia --help
Bounded, read-only Wikipedia lookups

Usage: wikipedia <COMMAND>

Commands:
  search   Start here: find page titles that match a phrase
  page     Read one page's compact lead, by an exact title from search
  outline  List one page's sections, each with an index for section
  section  Read exactly one section of a page, by an index copied from outline
  links    List a page's article links, one bounded page at a time
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version

Start with search, read a lead with page, and go deeper with outline, then section:
  wikipedia search Ada Lovelace
  wikipedia page --title "Ada Lovelace"
  wikipedia outline --title "Ada Lovelace"
  wikipedia section --title "Ada Lovelace" --section-index 1
```

`search` takes its phrase as operands, so quoting is optional. Every other verb takes `--title`, copied from an earlier result.

```sh
wikipedia search Ada Lovelace --limit 3
# {"results":[{"title":"Ada Lovelace",...}],"next_cursor":"CURSOR","pagination_capped":false}
wikipedia search Ada Lovelace --limit 3 --cursor CURSOR

wikipedia page --title "Ada Lovelace" --max-chars 1200
```

Outline, then section: copy one `sections[].index` from a fresh outline into `--section-index`, unchanged. It is a string on the wire, and `section` checks it against the page's current revision before reading that section from the same revision.

```sh
wikipedia outline --title "Ada Lovelace" --max-sections 20
# {"sections":[{"index":"1","title":"Biography",...}],...}
wikipedia section --title "Ada Lovelace" --section-index 1 --max-chars 3000
```

Links paginate like search:

```sh
wikipedia links --title "Ada Lovelace"
# {"links":[...],"next_cursor":"LINK_CURSOR","pagination_capped":false}
wikipedia links --title "Ada Lovelace" --cursor LINK_CURSOR
```

Keep the query or title, `--language`, and `--limit` identical when passing `next_cursor` to the next call. `--language` defaults to `en` and is checked against a dated, checked-in list of active Wikipedia editions.

Each verb's page:

```console
$ wikipedia search --help
Start here: find page titles that match a phrase

Usage: wikipedia search [OPTIONS] <QUERY>...

Arguments:
  <QUERY>...  What to look for; several words are joined with single spaces

Options:
      --language <CODE>  Wikipedia edition: en, de, fr, simple, or another active language code [default: en]
      --limit <N>        Most titles to return, 1 to 10 [default: 5]
      --cursor <CURSOR>  The next_cursor of the previous identical search, unchanged
  -h, --help             Print help

$ wikipedia page --help
Read one page's compact lead, by an exact title from search

Usage: wikipedia page [OPTIONS] --title <TITLE>

Options:
      --title <TITLE>    The exact title, as search returned it
      --language <CODE>  Wikipedia edition: en, de, fr, simple, or another active language code [default: en]
      --max-chars <N>    Most characters of lead text, 1 to 1200 [default: 900]
  -h, --help             Print help

$ wikipedia outline --help
List one page's sections, each with an index for section

Usage: wikipedia outline [OPTIONS] --title <TITLE>

Options:
      --title <TITLE>     The page's title, from search or page
      --language <CODE>   Wikipedia edition: en, de, fr, simple, or another active language code [default: en]
      --max-sections <N>  Most sections to list, 1 to 60 [default: 30]
  -h, --help              Print help

$ wikipedia section --help
Read exactly one section of a page, by an index copied from outline

Usage: wikipedia section [OPTIONS] --title <TITLE> --section-index <INDEX>

Options:
      --title <TITLE>          The title outline was run with
      --section-index <INDEX>  One sections[].index from outline, copied unchanged
      --language <CODE>        Wikipedia edition: en, de, fr, simple, or another active language code [default: en]
      --max-chars <N>          Most characters of section text, 1 to 8000 [default: 3000]
  -h, --help                   Print help

$ wikipedia links --help
List a page's article links, one bounded page at a time

Usage: wikipedia links [OPTIONS] --title <TITLE>

Options:
      --title <TITLE>    The page whose article links to list
      --language <CODE>  Wikipedia edition: en, de, fr, simple, or another active language code [default: en]
      --limit <N>        Most links to return, 1 to 20 [default: 20]
      --cursor <CURSOR>  The next_cursor of the previous identical links call, unchanged
  -h, --help             Print help
```

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
    constraints: *wikipediaOneRequest
  wikipedia_outline:
    provider: mediawiki
    effect: read-only
    risk: Low
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
  wikipedia_links:
    provider: mediawiki
    effect: read-only
    risk: Low
    constraints: *wikipediaOneRequest
  wikipedia_section:
    provider: mediawiki
    effect: read-only
    risk: Low
    constraints:
      timeoutMs: 10000
      maxOutputBytes: 16384
      http:
        allowedHosts: [en.wikipedia.org]
        allowedMethods: [GET]
        maxRequests: 3
        maxRequestBytes: 16384
        maxResponseBytes: 1048576
        allowPlaintextLoopback: false
```

Add `de.wikipedia.org` (or another checked-in edition) explicitly to every capability that may use it. Do not configure credentials: the guest never sets `authorization`, and Wikipedia reads are public. Add ordinary deny-by-default Cedar permits for only the principals and capability actions that should use these tools. Constraint sets and Cedar stay per capability: `wikipedia section …` authorizes as `wikipedia_section`.

HTTP imports are linked only by the broker. Nothing else links them, so the component is inert outside a broker that supplies `dekopon:http/client`.

## Build and validate

The release toolchain and encoder are exact pins:

```console
rustup toolchain install 1.98.1 --profile minimal --component clippy --component rustfmt
rustup target add wasm32-unknown-unknown --toolchain 1.98.1
rustup run 1.98.1 cargo install wasm-tools --version 1.259.0 --locked
cargo test --locked --package dekopon-mediawiki-provider
WASM_TOOLS_VERSION=1.259.0 ../provider-workflows/build.sh
```

The shared [`dekopon-agents/provider-workflows`](https://github.com/dekopon-agents/provider-workflows) repository owns `build.sh` (checked out next to this repository) with the proven deterministic metadata normalization, source/Cargo/sysroot path remapping, and path scan; it uses the checkout's ordinary `target/` and never redirects Cargo to a shared build directory. The shared `ci / validate` gate runs formatting, clippy, `cargo deny`, and reproducibility from two independent checkouts; the shared release workflow builds, attests, and publishes on a `v*` tag.

## Manual broker smoke checklist

After installing the built component under a constrained `dekopon-brokerd`, verify English and German search; `Ada Lovelace`; redirects through `NYC`; disambiguation through `Mercury`; a deliberately missing title; outline → copied section index; and two links pages using the returned cursor. Record actual evidence in [`AUTHORING.md`](AUTHORING.md); none is claimed before it is run.

## Design notes

Implementation and reusable provider-design lessons are in [`docs/provider-authoring-experience.md`](docs/provider-authoring-experience.md). The from-scratch chronological record is [`AUTHORING.md`](AUTHORING.md).

## License

MIT or Apache-2.0, at your option.
