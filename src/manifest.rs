use dekopon_provider_sdk::{
    EffectKind, ProviderApiVersion, ProviderCapability, ProviderManifest, RiskLevel,
};
use serde_json::{Value, json};

use crate::input::{
    DEFAULT_LINK_LIMIT, DEFAULT_OUTLINE_SECTIONS, DEFAULT_PAGE_CHARS, DEFAULT_SEARCH_LIMIT,
    DEFAULT_SECTION_CHARS, MAX_CURSOR_BYTES, MAX_LINK_LIMIT, MAX_OUTLINE_SECTIONS, MAX_PAGE_CHARS,
    MAX_SEARCH_LIMIT, MAX_SECTION_CHARS, MAX_TITLE_BYTES,
};

pub(crate) fn manifest() -> ProviderManifest {
    ProviderManifest {
        api_version: ProviderApiVersion::V1Alpha1,
        id: "mediawiki".parse().expect("static provider ID is valid"),
        description: "Five bounded read-only Wikipedia tools guiding search to a compact lead, outline, one section, and controlled links"
            .to_owned(),
        command_words: Vec::new(),
        capabilities: vec![
            read(
                "wikipedia_search",
                "Start here: find bounded Wikipedia page candidates, then pass one exact title to wikipedia_page for a compact overview",
                object_schema(
                    json!({
                        "query": {
                            "type": "string",
                            "minLength": 1,
                            "maxLength": 256,
                            "description": "Search phrase. Controls and insource: are rejected; no raw query passthrough."
                        },
                        "language": language_property(),
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": MAX_SEARCH_LIMIT,
                            "default": DEFAULT_SEARCH_LIMIT,
                            "description": "Maximum candidates from one API page."
                        },
                        "cursor": cursor_property("Cursor returned by the preceding identical wikipedia_search request; do not edit it."),
                    }),
                    &["query"],
                ),
            ),
            read(
                "wikipedia_page",
                "After search, read one compact canonical lead and identity; use wikipedia_outline for detail instead of requesting a whole article",
                object_schema(
                    json!({
                        "title": title_property("Exact title from wikipedia_search; redirects resolve through Wikipedia only."),
                        "language": language_property(),
                        "max_chars": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": MAX_PAGE_CHARS,
                            "default": DEFAULT_PAGE_CHARS,
                            "description": "Maximum Unicode characters in the compact lead."
                        },
                    }),
                    &["title"],
                ),
            ),
            read(
                "wikipedia_outline",
                "List a page's bounded table of contents; choose one returned index and pass it unchanged to wikipedia_section",
                object_schema(
                    json!({
                        "title": title_property("Canonical or redirecting Wikipedia title from search/page."),
                        "language": language_property(),
                        "max_sections": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": MAX_OUTLINE_SECTIONS,
                            "default": DEFAULT_OUTLINE_SECTIONS,
                            "description": "Maximum outline entries; no continuation is drained."
                        },
                    }),
                    &["title"],
                ),
            ),
            read(
                "wikipedia_section",
                "Retrieve exactly one bounded section selected from wikipedia_outline and pinned to the resolved revision; never dumps a whole article",
                object_schema(
                    json!({
                        "title": title_property("The same title used for the outline."),
                        "section_index": {
                            "type": "string",
                            "minLength": 1,
                            "maxLength": 32,
                            "description": "Copy one index exactly from wikipedia_outline; headings are not accepted as selectors."
                        },
                        "language": language_property(),
                        "max_chars": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": MAX_SECTION_CHARS,
                            "default": DEFAULT_SECTION_CHARS,
                            "description": "Maximum Unicode characters in the selected section plaintext."
                        },
                    }),
                    &["title", "section_index"],
                ),
            ),
            read(
                "wikipedia_links",
                "After reading a page, list one bounded page of main-namespace links for controlled follow-up; search or inspect selected links rather than spidering blindly",
                object_schema(
                    json!({
                        "title": title_property("Canonical or redirecting page whose article links should be listed."),
                        "language": language_property(),
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": MAX_LINK_LIMIT,
                            "default": DEFAULT_LINK_LIMIT,
                            "description": "Maximum main-namespace links from one API page."
                        },
                        "cursor": cursor_property("Cursor returned by the preceding identical wikipedia_links request; do not edit it."),
                    }),
                    &["title"],
                ),
            ),
        ],
    }
}

fn read(id: &str, description: &str, input_schema: Value) -> ProviderCapability {
    ProviderCapability {
        id: id.parse().expect("static capability ID is valid"),
        description: description.to_owned(),
        effect: EffectKind::ReadOnly,
        risk: RiskLevel::Low,
        input_schema,
    }
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn language_property() -> Value {
    json!({
        "type": "string",
        "pattern": "^[a-z][a-z0-9-]{0,15}$",
        "maxLength": 16,
        "default": "en",
        "description": "Active Wikipedia edition code from the provider's checked-in allowlist; defaults to en (for example: de, fr, simple, zh-min-nan)."
    })
}

fn title_property(description: &str) -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "maxLength": MAX_TITLE_BYTES,
        "description": description,
    })
}

fn cursor_property(description: &str) -> Value {
    json!({
        "type": "string",
        "maxLength": MAX_CURSOR_BYTES,
        "description": description,
    })
}

#[cfg(test)]
mod tests {
    use dekopon_provider_sdk::{EffectKind, RiskLevel};
    use serde_json::json;

    use super::manifest;

    #[test]
    fn manifest_has_exactly_the_approved_guided_surface() {
        let manifest = manifest();
        assert_eq!(manifest.id.as_str(), "mediawiki");
        assert!(manifest.command_words.is_empty());
        assert_eq!(
            manifest
                .capabilities
                .iter()
                .map(|capability| capability.id.as_str())
                .collect::<Vec<_>>(),
            [
                "wikipedia_search",
                "wikipedia_page",
                "wikipedia_outline",
                "wikipedia_section",
                "wikipedia_links",
            ]
        );
        for capability in &manifest.capabilities {
            assert_eq!(capability.effect, EffectKind::ReadOnly);
            assert_eq!(capability.risk, RiskLevel::Low);
            assert_eq!(capability.input_schema["type"], "object");
            assert_eq!(
                capability.input_schema["additionalProperties"],
                json!(false)
            );
            assert!(
                capability.description.contains("wikipedia_")
                    || capability.id.as_str() == "wikipedia_links"
            );
        }
    }

    #[test]
    fn every_schema_documents_the_natively_checked_language() {
        let manifest = manifest();
        for capability in manifest.capabilities {
            let language = &capability.input_schema["properties"]["language"];
            assert_eq!(language["default"], "en");
            assert_eq!(language["maxLength"], 16);
            assert!(
                language["description"]
                    .as_str()
                    .expect("description")
                    .contains("checked-in allowlist")
            );
        }
    }
}
