# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.8.0] - 2026-10-08

### Changed

- Move to SDK and testkit 0.38.0. The authoritative owner setting `providerSettings.mediawiki.baseUrl` selects one wiki/language at a time, defaulting to `https://en.wikipedia.org`; use `https://fr.wikipedia.org` for French.
- Preserve configured prefixes when joining `/w/api.php`. Page and section links use the same base with `/w/index.php?title=…`; proxies must expose `/w/index.php` alongside `/w/api.php` under that prefix.
- Bind cursors to the configured base; restart pagination after upgrading or changing wikis.

### Removed

- Remove per-call `language` inputs, `--language` flags, and the guest Wikipedia language/host allowlist without aliases. Broker destination and credential policy remains authoritative.

## [0.7.0] - 2026-10-04

### Changed

- Move to Dekopon SDK and testkit 0.33.0 and HTTP client 1.1.0; preserve bounded read-only Wikipedia behavior and typed stdio.

## [0.6.0] - 2026-10-03

### Changed

- Move to provider SDK 0.31.0 with typed stdio streams; keep the `wikipedia` command word and bounded read-only Wikipedia behavior.
- Rename capability IDs to `mediawiki.search`, `mediawiki.page`, `mediawiki.outline`, `mediawiki.section`, and `mediawiki.links`.
- Test real-component conformance and broker denials; retain fixed-authority HTTPS positive fixture coverage on the native path pending a testkit fixture for the component.

## [0.5.0] - 2026-09-20

### Changed

- Move to provider SDK 0.18.0 and HTTP 1.1.0; no caller-facing behavior changes.
