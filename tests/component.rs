use dekopon_mediawiki_provider::MediaWiki;
use dekopon_provider_sdk::provider::Response;
use dekopon_provider_sdk::{CommandRunOutcome, EffectKind, RiskLevel, provider};
use dekopon_provider_sdk_testkit::{Harness, HarnessError, HttpScript, Native, conformance};
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
        let properties = &capability.input_schema["properties"];
        assert!(
            properties["language"]["description"]
                .as_str()
                .is_some_and(|text| text.contains("Checked-in active Wikipedia edition")),
            "{id}: language allowlist guidance: {:?}",
            properties["language"]
        );
        if matches!(id, "mediawiki.search" | "mediawiki.links") {
            assert!(
                properties["cursor"]["description"]
                    .as_str()
                    .is_some_and(|text| text.contains("unchanged")),
                "{id}: exact cursor copy guidance"
            );
        }
        if id == "mediawiki.search" {
            assert!(
                properties["query"]["description"]
                    .as_str()
                    .is_some_and(|text| text.contains("insource:")),
                "search query must warn about forbidden syntax"
            );
        }
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
    // The existing pre-migration API fixture test pins this exact snippet and GET;
    // the typed CLI proposal and a direct authorized call must produce identical stdout.
    assert_eq!(
        value["results"][0]["snippet"],
        "Ada Lovelace was a mathematician & writer."
    );
    let CommandRunOutcome::Proposed {
        capability,
        input,
        secret_use,
    } = provider::command::<MediaWiki>(
        &[
            "search".into(),
            "Ada Lovelace".into(),
            "--limit".into(),
            "2".into(),
        ],
        false,
    )
    else {
        panic!("search must propose an authorized call")
    };
    assert_eq!(capability.as_str(), "mediawiki.search");
    assert!(secret_use.is_none());
    let from_cli = Native::<MediaWiki>::new().http(HttpScript::new(
        "en.wikipedia.org",
        "GET",
        Response {
            status: 200,
            headers: vec![],
            body: include_bytes!("fixtures/search-page-1.json").to_vec(),
        },
    ));
    let proposed = from_cli.call(capability.as_str(), &input.to_string());
    assert_eq!(proposed.status, 0, "{}", proposed.stderr);
    assert_eq!(
        proposed.stdout, output.stdout,
        "old fixture-backed direct and typed CLI paths must agree"
    );
    assert_eq!(from_cli.requests().len(), 1);
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

#[test]
fn real_component_refuses_ephemeral_fixture_for_fixed_https_authority() {
    let response = Response {
        status: 200,
        headers: vec![],
        body: include_bytes!("fixtures/search-page-1.json").to_vec(),
    };
    // The testkit authorizes en.wikipedia.org:<dynamic port>, not the provider's
    // en.wikipedia.org:443. Refusal is the safe behavior, not a positive HTTP smoke.
    let harness = Harness::<MediaWiki>::get(component()).http(HttpScript::new(
        "en.wikipedia.org",
        "GET",
        response,
    ));
    let origin = harness
        .origin()
        .expect("ephemeral HTTPS fixture")
        .to_owned();
    assert_ne!(origin, "https://en.wikipedia.org");
    let result = harness.call(
        "mediawiki.search",
        json!({"query":"Ada Lovelace","limit":2}),
    );
    match result {
        Err(HarnessError::Invocation(failure)) => {
            assert!(
                matches!(
                    failure.error.as_ref(),
                    dekopon_broker_host::BrokerHostError::HostCallRejected {
                        reason: "denied",
                        ..
                    }
                ),
                "{failure}"
            );
            assert!(
                failure.http_calls.is_empty(),
                "denial must precede any HTTP effect"
            );
        }
        Ok(output) => {
            assert_ne!(output.status, 0);
            assert!(output.stdout.is_empty());
            assert!(
                output.http_calls.is_empty(),
                "denial must precede any HTTP effect"
            );
        }
        Err(other) => panic!("unexpected fixture failure: {other}"),
    }
}
