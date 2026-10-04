# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
