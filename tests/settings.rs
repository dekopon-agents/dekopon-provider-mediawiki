use dekopon_mediawiki_provider::MediaWiki;
use dekopon_provider_sdk::provider::{self, Response};
use dekopon_provider_sdk_testkit::{HttpScript, Native};
use serde_json::{Value, json};

fn calls() -> [(&'static str, Value); 5] {
    [
        ("search", json!({"query":"Ada"})),
        ("page", json!({"title":"Ada"})),
        ("outline", json!({"title":"Ada"})),
        ("section", json!({"title":"Ada", "section_index":"1"})),
        ("links", json!({"title":"Ada"})),
    ]
}

#[test]
fn malformed_owner_settings_fail_before_requests() {
    for settings in [
        json!({"baseUrl":"https://fixture.example.test?x=1"}),
        json!({"baseUrl":"https://user@fixture.example.test"}),
        json!({"baseUrl":"https://fixture.example.test#fragment"}),
        json!({"baseUrl":"ftp://fixture.example.test"}),
        json!({"baseUrl":"fixture.example.test"}),
        json!({"baseUrl":42}),
        json!({"baseUrl":null}),
        json!({"base_url":"https://fixture.example.test"}),
        json!({"baseUrl":"https://fixture.example.test", "unknown":true}),
    ] {
        for (name, input) in calls() {
            let native = Native::<MediaWiki>::new().settings(settings.clone());
            let output = native.call(&format!("mediawiki.{name}"), &input.to_string());
            assert_ne!(output.status, 0, "{name}: {settings}");
            assert!(output.stderr.contains("settings"), "{}", output.stderr);
            assert!(output.stdout.is_empty());
            assert!(native.requests().is_empty());
        }
    }
}

#[test]
fn schemas_and_direct_calls_refuse_removed_origin_controls() {
    let manifest = provider::manifest::<MediaWiki>().unwrap();
    for (name, input) in calls() {
        let id = format!("mediawiki.{name}");
        let schema = &manifest
            .capabilities
            .iter()
            .find(|cap| cap.id.as_str() == id)
            .unwrap()
            .input_schema;
        assert_eq!(schema["additionalProperties"], false);
        for key in ["language", "baseUrl", "base_url", "endpoint", "url"] {
            assert!(schema["properties"].get(key).is_none());
            let mut input = input.clone();
            input[key] = json!("https://fixture.example.test");
            let native = Native::<MediaWiki>::new();
            let output = native.call(&id, &input.to_string());
            assert_ne!(output.status, 0);
            assert!(native.requests().is_empty());
        }
        let words: Vec<_> = match name {
            "search" => vec!["search", "Ada", "--language", "fr"],
            "section" => vec![
                "section",
                "--title",
                "Ada",
                "--section-index",
                "1",
                "--language",
                "fr",
            ],
            name => vec![name, "--title", "Ada", "--language", "fr"],
        }
        .into_iter()
        .map(str::to_owned)
        .collect();
        let dekopon_provider_sdk::CommandRunOutcome::Rendered {
            stdout,
            stderr,
            status,
        } = provider::command::<MediaWiki>(&words, false)
        else {
            panic!("removed language flag must render a usage error");
        };
        assert_eq!(status, 2);
        assert!(stdout.is_empty());
        assert!(stderr.contains("unexpected argument '--language'"));
    }
}

fn response(body: &[u8]) -> Response {
    Response {
        status: 200,
        headers: vec![],
        body: body.to_vec(),
    }
}

#[test]
fn content_links_use_the_selected_wiki_and_prefix() {
    for (base, host) in [
        ("https://fr.wikipedia.org", "fr.wikipedia.org"),
        (
            "https://fixture.example.test/prefix",
            "fixture.example.test",
        ),
    ] {
        let native = Native::<MediaWiki>::new()
            .settings(json!({"baseUrl":base}))
            .http(HttpScript::new(
                host,
                "GET",
                response(include_bytes!("fixtures/page-redirect.json")),
            ));
        let output = native.call("mediawiki.page", r#"{"title":"NYC"}"#);
        assert_eq!(output.status, 0, "{}", output.stderr);
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["url"],
            format!("{base}/w/index.php?title=New_York_City")
        );
        assert_eq!(native.requests().len(), 1);
        assert_eq!(
            native.requests()[0].uri,
            format!(
                "{base}/w/api.php?action=query&format=json&formatversion=2&errorformat=plaintext&maxlag=5&prop=extracts%7Cdescription%7Cinfo%7Cpageprops&redirects=1&converttitles=1&exintro=1&explaintext=1&exlimit=1&titles=NYC"
            )
        );
    }
}

#[test]
fn cursors_cannot_be_reused_after_changing_the_owner_base() {
    let native = Native::<MediaWiki>::new().http(HttpScript::new(
        "en.wikipedia.org",
        "GET",
        response(include_bytes!("fixtures/search-page-1.json")),
    ));
    let output = native.call("mediawiki.search", r#"{"query":"Ada","limit":2}"#);
    assert_eq!(output.status, 0, "{}", output.stderr);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let changed =
        Native::<MediaWiki>::new().settings(json!({"baseUrl":"https://fr.wikipedia.org"}));
    let output = changed.call(
        "mediawiki.search",
        &json!({"query":"Ada","limit":2,"cursor":value["next_cursor"]}).to_string(),
    );
    assert_ne!(output.status, 0);
    assert!(output.stderr.contains("cursor"));
    assert!(changed.requests().is_empty());
}
