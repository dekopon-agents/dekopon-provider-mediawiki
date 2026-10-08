use dekopon_mediawiki_provider::MediaWiki;
use dekopon_provider_sdk::provider::{Header, Response};
use dekopon_provider_sdk_testkit::{HttpScript, Native};
use serde_json::{Value, json};

#[test]
fn authored_search_cassette_replays_default_and_prefixed_bases() {
    let exchange: Value = serde_json::from_str(include_str!(
        "cassettes/mediawiki/0001-GET-w-api-search.json"
    ))
    .expect("cassette v1");
    assert_eq!(exchange["version"], 1);
    for (setting, base, host) in [
        (None, "https://en.wikipedia.org", "en.wikipedia.org"),
        (
            Some("https://fixture.example.test/prefix/"),
            "https://fixture.example.test/prefix",
            "fixture.example.test",
        ),
        (
            Some("https://fr.wikipedia.org"),
            "https://fr.wikipedia.org",
            "fr.wikipedia.org",
        ),
    ] {
        let mut native = Native::<MediaWiki>::new().http(HttpScript::new(
            host,
            "GET",
            Response {
                status: exchange["response"]["status"]
                    .as_u64()
                    .unwrap()
                    .try_into()
                    .unwrap(),
                headers: vec![Header::text("content-type", "application/json").unwrap()],
                body: serde_json::to_vec(&exchange["response"]["body"]["json"]).unwrap(),
            },
        ));
        if let Some(base) = setting {
            native = native.settings(json!({"baseUrl": base}));
        }
        let output = native.call(
            "mediawiki.search",
            &json!({"query": "Ada & café", "limit": 2}).to_string(),
        );
        assert_eq!(output.status, 0, "{}", output.stderr);
        let sent = native.requests();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].method, exchange["request"]["method"]);
        assert_eq!(
            sent[0].uri,
            format!(
                "{base}{}?{}",
                exchange["request"]["path"].as_str().unwrap(),
                exchange["request"]["query"].as_str().unwrap()
            )
        );
        assert!(sent[0].body.is_empty());
        let accept = sent[0]
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case("accept"))
            .unwrap();
        assert_eq!(
            accept.value,
            exchange["request"]["headers"]["accept"]
                .as_str()
                .unwrap()
                .as_bytes()
        );
        assert!(
            !sent[0]
                .headers
                .iter()
                .any(|header| header.name.eq_ignore_ascii_case("authorization"))
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            result["results"],
            json!([
                {"page_id": 974, "title": "Ada Lovelace", "snippet": "Ada Lovelace was a mathematician & writer.", "word_count": 10102, "modified": "2026-08-19T11:48:44Z"},
                {"page_id": 71803512, "title": "Ada Lovelace (microarchitecture)", "snippet": "An Nvidia GPU microarchitecture.", "word_count": 1843, "modified": "2026-05-08T14:05:16Z"}
            ])
        );
        assert_eq!(result["total_hits"], 658);
        assert!(result["next_cursor"].is_string());
        assert_eq!(result["pagination_capped"], false);
    }
}
