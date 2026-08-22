//! Five narrow, bounded Wikipedia capabilities for Dekopon.
//!
//! The component accepts no endpoint or credential input. It constructs one allowlisted Wikipedia
//! Action API origin, performs at most one request (two for a revision-pinned section), and projects
//! only compact structured plaintext. HTTP authority remains broker-owned.

use dekopon_provider_http::{HttpError, Request, Response};
use dekopon_provider_sdk::{CapabilityId, Provider, ProviderError, ProviderManifest};
use serde_json::Value;

mod api;
mod budget;
mod cursor;
mod error;
mod html_text;
mod input;
mod manifest;

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
        "wikipedia_search" => api::search(input::parse_search(input)?, send),
        "wikipedia_page" => api::page(input::parse_page(input)?, send),
        "wikipedia_outline" => api::outline(input::parse_outline(input)?, send),
        "wikipedia_section" => api::section(input::parse_section(input)?, send),
        "wikipedia_links" => api::links(input::parse_links(input)?, send),
        _ => Err(error::unknown_capability()),
    }
}

dekopon_provider_sdk::export_provider_with_bindings!(MediaWiki, bindings);

#[cfg(test)]
pub(crate) mod testutil {
    pub(crate) fn capability(value: &str) -> dekopon_provider_sdk::CapabilityId {
        value.parse().expect("valid capability fixture")
    }
}

#[cfg(test)]
mod tests {
    use dekopon_provider_sdk::Provider;
    use serde_json::Value;

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
    }
}
