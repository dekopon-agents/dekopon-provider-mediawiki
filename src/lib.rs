//! Five narrow, bounded Wikipedia capabilities through the `wikipedia` shell word.
//! HTTP authority remains broker-owned; results go only to stdout.

use dekopon_provider_sdk::provider::{Capability, Http, Proposal, Provider, Stdout, Usage};
use dekopon_provider_sdk::{EffectKind, RiskLevel};
use std::io::Write;

mod api;
mod budget;
mod commands;
mod cursor;
mod error;
mod html_text;
mod input;

#[cfg(test)]
fn invoke_with<F>(
    capability: &dekopon_provider_sdk::CapabilityId,
    input: serde_json::Value,
    mut send: F,
) -> Result<serde_json::Value, error::ProviderError>
where
    F: FnMut(
        dekopon_provider_sdk::provider::Request,
    ) -> Result<
        dekopon_provider_sdk::provider::Response,
        dekopon_provider_sdk::provider::HttpError,
    >,
{
    let send = &mut send;
    match capability.as_str() {
        "mediawiki.search" => api::search(input::parse_search(input)?, send),
        "mediawiki.page" => api::page(input::parse_page(input)?, send),
        "mediawiki.outline" => api::outline(input::parse_outline(input)?, send),
        "mediawiki.section" => api::section(input::parse_section(input)?, send),
        "mediawiki.links" => api::links(input::parse_links(input)?, send),
        _ => Err(error::unknown_capability()),
    }
}

pub struct MediaWiki;
pub struct Search;
pub struct Page;
pub struct Outline;
pub struct Section;
pub struct Links;

#[cfg(test)]
const SEARCH: &str = "mediawiki.search";
#[cfg(test)]
const PAGE: &str = "mediawiki.page";
#[cfg(test)]
const OUTLINE: &str = "mediawiki.outline";
#[cfg(test)]
const SECTION: &str = "mediawiki.section";
#[cfg(test)]
const LINKS: &str = "mediawiki.links";

impl Provider for MediaWiki {
    const ID: &'static str = "mediawiki";
    const COMMAND_WORDS: &'static [&'static str] = &["wikipedia"];
    const DESCRIPTION: &'static str = "Five bounded read-only Wikipedia tools guiding search to a compact lead, outline, one section, and controlled links";
    type Args = commands::Wikipedia;
    type Capabilities = (Search, Page, Outline, Section, Links);

    fn propose(args: Self::Args, stdin_piped: bool) -> Result<Proposal<Self>, Usage> {
        commands::propose(args, stdin_piped)
    }
}

macro_rules! capability {
    ($kind:ident, $name:literal, $description:literal, $input:ty, $parse:path, $run:path) => {
        impl Capability for $kind {
            type Provider = MediaWiki;
            const NAME: &'static str = $name;
            const DESCRIPTION: &'static str = $description;
            const EFFECT: EffectKind = EffectKind::ReadOnly;
            const RISK: RiskLevel = RiskLevel::Low;
            type Input = $input;
            type Needs = Http;
            type Error = error::ProviderError;

            fn run(input: Self::Input, http: Http, out: &mut Stdout) -> Result<(), Self::Error> {
                // Validate direct calls and proposals identically before any HTTP request.
                let input = serde_json::to_value(input).map_err(|_| error::invalid_input())?;
                let input = $parse(input)?;
                let output = $run(input, &mut |request| http.send(request))?;
                let bytes = serde_json::to_vec(&output).map_err(|_| error::upstream_error())?;
                out.write_all(&bytes).map_err(|_| error::output_closed())?;
                out.write_all(b"\n").map_err(|_| error::output_closed())?;
                Ok(())
            }
        }
    };
}

capability!(
    Search,
    "search",
    "Start here: find bounded Wikipedia page candidates, then pass one exact title to `wikipedia page --title` for a compact overview",
    input::SearchInput,
    input::parse_search,
    api::search
);
capability!(
    Page,
    "page",
    "After search, read one compact canonical lead and identity; use `wikipedia outline` for detail instead of requesting a whole article",
    input::PageInput,
    input::parse_page,
    api::page
);
capability!(
    Outline,
    "outline",
    "List a page's bounded table of contents; choose one returned index and pass it unchanged to `wikipedia section --section-index`",
    input::OutlineInput,
    input::parse_outline,
    api::outline
);
capability!(
    Section,
    "section",
    "Retrieve exactly one bounded section selected from `wikipedia outline` and pinned to the resolved revision; never dumps a whole article",
    input::SectionInput,
    input::parse_section,
    api::section
);
capability!(
    Links,
    "links",
    "After reading a page, list one bounded page of main-namespace links for controlled follow-up",
    input::LinksInput,
    input::parse_links,
    api::links
);

#[allow(unsafe_code)]
mod export {
    dekopon_provider_sdk::export!(super::MediaWiki);
}

#[cfg(test)]
pub(crate) mod testutil {
    pub(crate) fn capability(value: &str) -> dekopon_provider_sdk::CapabilityId {
        let value = match value {
            "wikipedia_search" => "mediawiki.search",
            "wikipedia_page" => "mediawiki.page",
            "wikipedia_outline" => "mediawiki.outline",
            "wikipedia_section" => "mediawiki.section",
            "wikipedia_links" => "mediawiki.links",
            other => other,
        };
        value.parse().expect("valid capability fixture")
    }
}

#[cfg(test)]
mod tests {
    use super::MediaWiki;
    use dekopon_provider_sdk::provider;
    use serde_json::json;

    #[test]
    fn typed_manifest_has_five_broker_scoped_read_only_capabilities() {
        let manifest = provider::manifest::<MediaWiki>().expect("valid typed manifest");
        assert_eq!(manifest.id.as_str(), "mediawiki");
        assert_eq!(manifest.command_words, ["wikipedia"]);
        assert_eq!(manifest.capabilities.len(), 5);
        assert_eq!(
            manifest
                .capabilities
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            [
                "mediawiki.search",
                "mediawiki.page",
                "mediawiki.outline",
                "mediawiki.section",
                "mediawiki.links"
            ]
        );
        for cap in &manifest.capabilities {
            assert_eq!(cap.effect, dekopon_provider_sdk::EffectKind::ReadOnly);
            assert_eq!(cap.risk, dekopon_provider_sdk::RiskLevel::Low);
            assert_eq!(cap.input_schema["additionalProperties"], json!(false));
        }
    }
}
