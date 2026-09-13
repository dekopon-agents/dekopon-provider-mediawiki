//! Five narrow, bounded Wikipedia capabilities for Dekopon, reached through the `wikipedia` command
//! word.
//!
//! The component accepts no endpoint or credential input. It constructs one allowlisted Wikipedia
//! Action API origin, performs one request (two for an outline, three for a revision-pinned
//! section), and projects only compact structured plaintext. HTTP authority remains broker-owned.
//!
//! `wikipedia search|page|outline|section|links` is parsed inside the guest by `commands`, which
//! turns a well-formed argv into exactly the input `invoke` accepts.

use dekopon_provider_http::{HttpError, Request, Response};
use dekopon_provider_sdk::{CapabilityId, CommandRun, Provider, ProviderError, ProviderManifest};
use serde_json::Value;

mod api;
mod budget;
mod commands;
mod cursor;
mod error;
mod html_text;
mod input;
mod manifest;

/// Finds candidate page titles.
pub(crate) const SEARCH: &str = "wikipedia_search";
/// Reads one page's compact lead.
pub(crate) const PAGE: &str = "wikipedia_page";
/// Lists one page's sections.
pub(crate) const OUTLINE: &str = "wikipedia_outline";
/// Reads one revision-pinned section.
pub(crate) const SECTION: &str = "wikipedia_section";
/// Lists one page of a page's main-namespace links.
pub(crate) const LINKS: &str = "wikipedia_links";
/// The command word this provider contributes to the sandboxed shell. Separator-free, so
/// `dekopon-core` never mistakes it for a capability identifier.
pub(crate) const COMMAND_WORD: &str = "wikipedia";

mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "provider",
        generate_all,
        pub_export_macro: true,
    });
}

struct MediaWiki;

impl Provider for MediaWiki {
    fn manifest() -> ProviderManifest {
        manifest::manifest()
    }

    fn invoke(capability: &CapabilityId, input: Value) -> Result<Value, ProviderError> {
        invoke_with(capability, input, dekopon_provider_http::send)
    }

    fn run_command(argv: &[String], stdin: Option<&str>) -> Result<CommandRun, ProviderError> {
        commands::run(argv, stdin)
    }
}

fn invoke_with<F>(
    capability: &CapabilityId,
    input: Value,
    mut send: F,
) -> Result<Value, ProviderError>
where
    F: FnMut(Request) -> Result<Response, HttpError>,
{
    let send: &mut dyn FnMut(Request) -> Result<Response, HttpError> = &mut send;
    match capability.as_str() {
        SEARCH => api::search(input::parse_search(input)?, send),
        PAGE => api::page(input::parse_page(input)?, send),
        OUTLINE => api::outline(input::parse_outline(input)?, send),
        SECTION => api::section(input::parse_section(input)?, send),
        LINKS => api::links(input::parse_links(input)?, send),
        _ => Err(error::unknown_capability()),
    }
}

dekopon_provider_sdk::export_provider_with_cli!(MediaWiki, bindings);

#[cfg(test)]
pub(crate) mod testutil {
    pub(crate) fn capability(value: &str) -> dekopon_provider_sdk::CapabilityId {
        value.parse().expect("valid capability fixture")
    }
}

#[cfg(test)]
mod tests {
    use dekopon_provider_sdk::Provider;
    use serde_json::{Value, json};

    use super::MediaWiki;

    #[test]
    fn mirrored_wit_exactly_matches_the_pinned_rust_crates() {
        assert_eq!(
            include_str!("../wit/deps/provider.wit"),
            dekopon_provider_sdk::PROVIDER_WIT
        );
        assert_eq!(
            include_str!("../wit/deps/http.wit"),
            dekopon_provider_http::HTTP_WIT
        );
    }

    #[test]
    fn manifest_snapshot() {
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&MediaWiki::manifest()).expect("manifest serializes")
        );
        let expected = include_str!("../tests/fixtures/manifest.json");
        assert_eq!(actual, expected);

        let decoded: Value = serde_json::from_str(expected).expect("snapshot is JSON");
        assert_eq!(decoded["capabilities"].as_array().expect("array").len(), 5);
        assert_eq!(decoded["commandWords"], json!(["wikipedia"]));
    }
}
