use dekopon_mediawiki_provider::MediaWiki;
use dekopon_provider_sdk::provider::Response;
use dekopon_provider_sdk::{CommandRunOutcome, EffectKind, RiskLevel, provider};
use dekopon_provider_sdk_testkit::{Harness, HttpScript, Native, conformance};
use serde_json::json;
use std::path::PathBuf;

fn component() -> PathBuf {
    std::env::var_os("DEKOPON_PROVIDER_COMPONENT")
        .expect("DEKOPON_PROVIDER_COMPONENT must point to the fresh component")
        .into()
}

#[test]
fn real_component_conforms_to_typed_manifest_and_stdio() {
    let component = component();
    conformance::<MediaWiki>(&component)
        .expect("real component matches typed declaration, closed schemas, stdio and HTTP imports");
    let manifest = provider::manifest::<MediaWiki>().expect("manifest");
    assert_eq!(manifest.command_words, ["wikipedia"]);
    let ids = [
        "mediawiki.search",
        "mediawiki.page",
        "mediawiki.outline",
        "mediawiki.section",
        "mediawiki.links",
    ];
    for (capability, id) in manifest.capabilities.iter().zip(ids) {
        assert_eq!(capability.id.as_str(), id);
        assert_eq!(capability.effect, EffectKind::ReadOnly);
        assert_eq!(capability.risk, RiskLevel::Low);
        assert_eq!(capability.input_schema["additionalProperties"], false);
    }
    for piped in [false, true] {
        let CommandRunOutcome::Proposed {
            capability,
            input,
            secret_use,
        } = provider::command::<MediaWiki>(&["search".into(), "Ada".into()], piped)
        else {
            panic!("search proposes")
        };
        assert_eq!(capability.as_str(), "mediawiki.search");
        assert_eq!(input["query"], "Ada");
        assert!(secret_use.is_none());
    }
    // A direct call without HTTP authority must not fetch Wikipedia or emit content.
    let denied =
        Harness::<MediaWiki>::get(&component).call("mediawiki.search", json!({"query":"Ada"}));
    assert!(
        denied.is_err(),
        "HTTP capability without a broker HTTP grant must be refused"
    );
}

#[test]
fn native_authorized_http_writes_bounded_search_snippets_to_stdout() {
    let native = Native::<MediaWiki>::new().http(HttpScript::new(
        "en.wikipedia.org",
        "GET",
        Response {
            status: 200,
            headers: vec![],
            body: include_bytes!("fixtures/search-page-1.json").to_vec(),
        },
    ));
    let output = native.call("mediawiki.search", r#"{"query":"Ada Lovelace","limit":2}"#);
    assert_eq!(output.status, 0, "{}", output.stderr);
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("one JSON line");
    assert_eq!(value["results"][0]["title"], "Ada Lovelace");
    assert!(
        value["results"][0]["snippet"]
            .as_str()
            .unwrap()
            .contains("mathematician")
    );
    assert!(output.stdout.len() <= 14_001);
    let requests = native.requests();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0]
            .uri
            .starts_with("https://en.wikipedia.org/w/api.php?")
    );
    assert!(
        requests[0]
            .headers
            .iter()
            .all(|header| header.name != "authorization")
    );

    let invalid =
        Native::<MediaWiki>::new().call("mediawiki.search", r#"{"query":"insource:secret"}"#);
    assert_ne!(invalid.status, 0);
    assert!(invalid.stdout.is_empty());
    assert!(!invalid.stderr.contains("secret"));
}
