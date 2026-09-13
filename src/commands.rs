//! The `wikipedia` command word: a small command-line program, rendered by the guest.
//!
//! `wikipedia --help`, each verb's `--help`, `--version`, and every usage error are answered here
//! and authorize nothing. A well-formed argv becomes a *proposal* naming one capability, carrying
//! exactly the snake_case input `invoke` accepts, and it travels the ordinary authorization path:
//! constraint-set lookup, Cedar, then the broker's HTTP engine. Each flag is the kebab-case
//! spelling of one input field, so `--max-chars 1200` proposes `"max_chars": 1200`. The shell
//! translates nothing, which is what retires the camelCase rewrite `invoke` could not read.
//!
//! clap checks what an argv alone can know — a required flag, an integer inside its native
//! ceiling — so a model gets a usage error naming the flag instead of an opaque `invalid_input`.
//! Everything else — the language allowlist, title and query bytes, section index and cursor
//! shapes — is still checked once, in `invoke`, against the input a direct call would send too.

use dekopon_provider_sdk::clap::builder::RangedU64ValueParser;
use dekopon_provider_sdk::clap::{self, Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use dekopon_provider_sdk::{CommandInvocation, CommandRun, ProviderError, cli};
use serde_json::{Value, json};

use crate::input::{
    DEFAULT_LANGUAGE, DEFAULT_LINK_LIMIT, DEFAULT_OUTLINE_SECTIONS, DEFAULT_PAGE_CHARS,
    DEFAULT_SEARCH_LIMIT, DEFAULT_SECTION_CHARS, MAX_LINK_LIMIT, MAX_OUTLINE_SECTIONS,
    MAX_PAGE_CHARS, MAX_SEARCH_LIMIT, MAX_SECTION_CHARS,
};
use crate::{COMMAND_WORD, LINKS, OUTLINE, PAGE, SEARCH, SECTION};

/// The guided path, printed under the top-level help.
const FLOW: &str = "\
Start with search, read a lead with page, and go deeper with outline, then section:
  wikipedia search Ada Lovelace
  wikipedia page --title \"Ada Lovelace\"
  wikipedia outline --title \"Ada Lovelace\"
  wikipedia section --title \"Ada Lovelace\" --section-index 1";

// The `wikipedia` tree, declared once and rendered by clap. Plain comments, not doc comments: clap
// renders a doc comment as the `about` line above `Usage:`.
#[derive(Parser)]
#[command(
    name = COMMAND_WORD,
    version,
    about = "Bounded, read-only Wikipedia lookups",
    after_help = FLOW
)]
struct Wikipedia {
    #[command(subcommand)]
    verb: Verb,
}

// Each verb proposes exactly one capability, named by the `const` the manifest declares, so a
// renamed capability is a compile error rather than an exit code discovered mid-session.
#[derive(Subcommand)]
enum Verb {
    /// Start here: find page titles that match a phrase
    Search(Search),
    /// Read one page's compact lead, by an exact title from search
    Page(Page),
    /// List one page's sections, each with an index for section
    Outline(Outline),
    /// Read exactly one section of a page, by an index copied from outline
    Section(Section),
    /// List a page's article links, one bounded page at a time
    Links(Links),
}

// `search` takes its phrase as operands, the way `gh search` and `brew search` do: the phrase is
// the whole point of the verb, and `wikipedia search Ada Lovelace` then works with or without
// quotes. Every other verb names a page, and that title is copied from an earlier result, so it
// is a flag. A title may start with a hyphen (`-ism`), so `--title` accepts one as its value.
#[derive(Args)]
struct Search {
    /// What to look for; several words are joined with single spaces
    #[arg(value_name = "QUERY", required = true)]
    query: Vec<String>,
    #[command(flatten)]
    edition: Edition,
    /// Most titles to return, 1 to 10
    #[arg(long, value_name = "N", default_value_t = DEFAULT_SEARCH_LIMIT, value_parser = from_one_to(MAX_SEARCH_LIMIT))]
    limit: usize,
    /// The next_cursor of the previous identical search, unchanged
    #[arg(long, value_name = "CURSOR")]
    cursor: Option<String>,
}

#[derive(Args)]
struct Page {
    /// The exact title, as search returned it
    #[arg(long, value_name = "TITLE", allow_hyphen_values = true)]
    title: String,
    #[command(flatten)]
    edition: Edition,
    /// Most characters of lead text, 1 to 1200
    #[arg(long, value_name = "N", default_value_t = DEFAULT_PAGE_CHARS, value_parser = from_one_to(MAX_PAGE_CHARS))]
    max_chars: usize,
}

#[derive(Args)]
struct Outline {
    /// The page's title, from search or page
    #[arg(long, value_name = "TITLE", allow_hyphen_values = true)]
    title: String,
    #[command(flatten)]
    edition: Edition,
    /// Most sections to list, 1 to 60
    #[arg(long, value_name = "N", default_value_t = DEFAULT_OUTLINE_SECTIONS, value_parser = from_one_to(MAX_OUTLINE_SECTIONS))]
    max_sections: usize,
}

// `--section-index` stays a string, on the flag and on the wire: it is copied out of outline's
// output, never computed, and `invoke` rejects anything outline could not have issued.
#[derive(Args)]
struct Section {
    /// The title outline was run with
    #[arg(long, value_name = "TITLE", allow_hyphen_values = true)]
    title: String,
    /// One sections[].index from outline, copied unchanged
    #[arg(long, value_name = "INDEX")]
    section_index: String,
    #[command(flatten)]
    edition: Edition,
    /// Most characters of section text, 1 to 8000
    #[arg(long, value_name = "N", default_value_t = DEFAULT_SECTION_CHARS, value_parser = from_one_to(MAX_SECTION_CHARS))]
    max_chars: usize,
}

#[derive(Args)]
struct Links {
    /// The page whose article links to list
    #[arg(long, value_name = "TITLE", allow_hyphen_values = true)]
    title: String,
    #[command(flatten)]
    edition: Edition,
    /// Most links to return, 1 to 20
    #[arg(long, value_name = "N", default_value_t = DEFAULT_LINK_LIMIT, value_parser = from_one_to(MAX_LINK_LIMIT))]
    limit: usize,
    /// The next_cursor of the previous identical links call, unchanged
    #[arg(long, value_name = "CURSOR")]
    cursor: Option<String>,
}

#[derive(Args)]
struct Edition {
    /// Wikipedia edition: en, de, fr, simple, or another active language code
    #[arg(long, value_name = "CODE", default_value = DEFAULT_LANGUAGE)]
    language: String,
}

/// An integer from 1 to `max`, the same native ceiling `invoke` enforces.
fn from_one_to(max: usize) -> RangedU64ValueParser<usize> {
    RangedU64ValueParser::new().range(1..=max as u64)
}

/// Runs one `wikipedia` argv.
pub(crate) fn run(argv: &[String], stdin: Option<&str>) -> Result<CommandRun, ProviderError> {
    cli::run_command(Wikipedia::command(), argv, stdin, dispatch)
}

/// Turns clap's matches into the proposal for the selected verb, spelled as `invoke`'s input.
///
/// Every field is sent with its default filled in, so the proposal on a trace is the complete
/// request. No verb reads piped input.
fn dispatch(
    matches: clap::ArgMatches,
    _stdin: Option<&str>,
) -> Result<CommandInvocation, ProviderError> {
    let wikipedia = Wikipedia::from_arg_matches(&matches)
        .map_err(|error| ProviderError::new("usage", error.to_string()))?;
    let (capability, input) = match wikipedia.verb {
        Verb::Search(search) => (
            SEARCH,
            with_cursor(
                json!({
                    "query": search.query.join(" "),
                    "language": search.edition.language,
                    "limit": search.limit,
                }),
                search.cursor,
            ),
        ),
        Verb::Page(page) => (
            PAGE,
            json!({
                "title": page.title,
                "language": page.edition.language,
                "max_chars": page.max_chars,
            }),
        ),
        Verb::Outline(outline) => (
            OUTLINE,
            json!({
                "title": outline.title,
                "language": outline.edition.language,
                "max_sections": outline.max_sections,
            }),
        ),
        Verb::Section(section) => (
            SECTION,
            json!({
                "title": section.title,
                "section_index": section.section_index,
                "language": section.edition.language,
                "max_chars": section.max_chars,
            }),
        ),
        Verb::Links(links) => (
            LINKS,
            with_cursor(
                json!({
                    "title": links.title,
                    "language": links.edition.language,
                    "limit": links.limit,
                }),
                links.cursor,
            ),
        ),
    };
    Ok(CommandInvocation {
        capability: capability.parse().expect("static capability ID"),
        input,
    })
}

/// Adds `cursor` only when one was given: the schema types it as a string, never null, and an
/// absent cursor asks for the first page.
fn with_cursor(mut input: Value, cursor: Option<String>) -> Value {
    if let Some(cursor) = cursor {
        input["cursor"] = Value::String(cursor);
    }
    input
}

#[cfg(test)]
mod tests {
    use dekopon_provider_http::Response;
    use dekopon_provider_sdk::{CommandInvocation, CommandRun, Provider};
    use serde_json::{Value, json};

    use super::run;
    use crate::input::{
        MAX_LINK_LIMIT, MAX_OUTLINE_SECTIONS, MAX_PAGE_CHARS, MAX_SEARCH_LIMIT, MAX_SECTION_CHARS,
    };
    use crate::{LINKS, MediaWiki, OUTLINE, PAGE, SEARCH, SECTION, invoke_with};

    const TOP_HELP: &str = r#"Bounded, read-only Wikipedia lookups

Usage: wikipedia <COMMAND>

Commands:
  search   Start here: find page titles that match a phrase
  page     Read one page's compact lead, by an exact title from search
  outline  List one page's sections, each with an index for section
  section  Read exactly one section of a page, by an index copied from outline
  links    List a page's article links, one bounded page at a time
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version

Start with search, read a lead with page, and go deeper with outline, then section:
  wikipedia search Ada Lovelace
  wikipedia page --title "Ada Lovelace"
  wikipedia outline --title "Ada Lovelace"
  wikipedia section --title "Ada Lovelace" --section-index 1
"#;

    const SEARCH_HELP: &str = "Start here: find page titles that match a phrase

Usage: wikipedia search [OPTIONS] <QUERY>...

Arguments:
  <QUERY>...  What to look for; several words are joined with single spaces

Options:
      --language <CODE>  Wikipedia edition: en, de, fr, simple, or another active language code [default: en]
      --limit <N>        Most titles to return, 1 to 10 [default: 5]
      --cursor <CURSOR>  The next_cursor of the previous identical search, unchanged
  -h, --help             Print help
";

    const PAGE_HELP: &str = "Read one page's compact lead, by an exact title from search

Usage: wikipedia page [OPTIONS] --title <TITLE>

Options:
      --title <TITLE>    The exact title, as search returned it
      --language <CODE>  Wikipedia edition: en, de, fr, simple, or another active language code [default: en]
      --max-chars <N>    Most characters of lead text, 1 to 1200 [default: 900]
  -h, --help             Print help
";

    const OUTLINE_HELP: &str = "List one page's sections, each with an index for section

Usage: wikipedia outline [OPTIONS] --title <TITLE>

Options:
      --title <TITLE>     The page's title, from search or page
      --language <CODE>   Wikipedia edition: en, de, fr, simple, or another active language code [default: en]
      --max-sections <N>  Most sections to list, 1 to 60 [default: 30]
  -h, --help              Print help
";

    const SECTION_HELP: &str = "Read exactly one section of a page, by an index copied from outline

Usage: wikipedia section [OPTIONS] --title <TITLE> --section-index <INDEX>

Options:
      --title <TITLE>          The title outline was run with
      --section-index <INDEX>  One sections[].index from outline, copied unchanged
      --language <CODE>        Wikipedia edition: en, de, fr, simple, or another active language code [default: en]
      --max-chars <N>          Most characters of section text, 1 to 8000 [default: 3000]
  -h, --help                   Print help
";

    const LINKS_HELP: &str = "List a page's article links, one bounded page at a time

Usage: wikipedia links [OPTIONS] --title <TITLE>

Options:
      --title <TITLE>    The page whose article links to list
      --language <CODE>  Wikipedia edition: en, de, fr, simple, or another active language code [default: en]
      --limit <N>        Most links to return, 1 to 20 [default: 20]
      --cursor <CURSOR>  The next_cursor of the previous identical links call, unchanged
  -h, --help             Print help
";

    /// The second page of `links-page-1`, as Wikipedia returns it once the cursor is replayed.
    const LINKS_PAGE_2: &str = r#"{"batchcomplete":true,"query":{"pages":[{"pageid":974,"ns":0,"title":"Ada Lovelace","links":[{"ns":0,"title":"Analytical Engine"}]}]}}"#;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    fn rendered(words: &[&str]) -> (String, String, u8) {
        let run = run(&argv(words), None).expect("clap answers are rendered, not declined");
        let CommandRun::Rendered {
            stdout,
            stderr,
            status,
        } = run
        else {
            panic!("expected rendered text for {words:?}, got {run:?}");
        };
        (stdout, stderr, status)
    }

    fn proposal(words: &[&str]) -> CommandInvocation {
        match run(&argv(words), None).expect("a well-formed argv proposes") {
            CommandRun::Proposal(invocation) => invocation,
            other => panic!("expected a proposal for {words:?}, got {other:?}"),
        }
    }

    fn fixture(name: &str) -> &'static str {
        match name {
            "search-page-1" => include_str!("../tests/fixtures/search-page-1.json"),
            "page-redirect" => include_str!("../tests/fixtures/page-redirect.json"),
            "outline-resolve" => include_str!("../tests/fixtures/outline-resolve.json"),
            "outline" => include_str!("../tests/fixtures/outline.json"),
            "section" => include_str!("../tests/fixtures/section.json"),
            "links-page-1" => include_str!("../tests/fixtures/links-page-1.json"),
            _ => panic!("unknown fixture {name}"),
        }
    }

    /// Runs one argv to its proposal, then the proposal through `invoke` against MediaWiki bodies
    /// served in order, returning the output and every request URI. This is the production path
    /// minus the broker, so it proves the input clap spells is the input `invoke` parses.
    fn round_trip(words: &[&str], bodies: &[&str]) -> (Value, Vec<String>) {
        let invocation = proposal(words);
        let mut uris = Vec::new();
        let output = invoke_with(&invocation.capability, invocation.input, |request| {
            let body = bodies
                .get(uris.len())
                .unwrap_or_else(|| panic!("{words:?}: unexpected request {}", request.uri));
            uris.push(request.uri);
            Ok(Response {
                status: 200,
                headers: Vec::new(),
                body: body.as_bytes().to_vec(),
            })
        })
        .unwrap_or_else(|error| panic!("{words:?}: {} {}", error.code(), error.message()));
        assert_eq!(uris.len(), bodies.len(), "{words:?}");
        (output, uris)
    }

    /// Every help page, byte for byte: this text is the whole manual a model reads.
    #[test]
    fn help_pages_are_pinned_and_render_on_stdout_at_status_zero() {
        for (verb, expected) in [
            (None, TOP_HELP),
            (Some("search"), SEARCH_HELP),
            (Some("page"), PAGE_HELP),
            (Some("outline"), OUTLINE_HELP),
            (Some("section"), SECTION_HELP),
            (Some("links"), LINKS_HELP),
        ] {
            for flag in ["--help", "-h"] {
                let words: Vec<&str> = verb.into_iter().chain([flag]).collect();
                let (stdout, stderr, status) = rendered(&words);
                assert_eq!(status, 0, "{words:?}");
                assert_eq!(stdout, expected, "{words:?}");
                assert!(stderr.is_empty(), "{words:?}: {stderr}");
            }
        }
    }

    #[test]
    fn version_renders_on_stdout_at_status_zero() {
        let (stdout, _, status) = rendered(&["--version"]);
        assert_eq!(status, 0);
        assert_eq!(stdout, format!("wikipedia {}\n", env!("CARGO_PKG_VERSION")));
    }

    /// clap renders each default from the constant `invoke` defaults to; the ceilings are prose,
    /// so the prose is checked against the constants clap and `invoke` both enforce.
    #[test]
    fn help_states_each_native_ceiling() {
        for (verb, ceiling) in [
            ("search", MAX_SEARCH_LIMIT),
            ("page", MAX_PAGE_CHARS),
            ("outline", MAX_OUTLINE_SECTIONS),
            ("section", MAX_SECTION_CHARS),
            ("links", MAX_LINK_LIMIT),
        ] {
            let (stdout, _, _) = rendered(&[verb, "--help"]);
            assert!(
                stdout.contains(&format!(", 1 to {ceiling} [default: ")),
                "{verb}: {stdout}"
            );
        }
    }

    /// A bare word, an unknown verb, and each verb missing a required argument are usage errors on
    /// stderr at status 2, whose usage line starts with the word the model typed.
    #[test]
    fn a_missing_argument_is_a_usage_error_on_stderr_at_status_two() {
        for (words, usage) in [
            (&[][..], "Usage: wikipedia <COMMAND>"),
            (&["bogus"][..], "Usage: wikipedia <COMMAND>"),
            (&["search"][..], "Usage: wikipedia search <QUERY>..."),
            (&["page"][..], "Usage: wikipedia page --title <TITLE>"),
            (&["outline"][..], "Usage: wikipedia outline --title <TITLE>"),
            (
                &["section", "--title", "Ada Lovelace"][..],
                "Usage: wikipedia section --title <TITLE> --section-index <INDEX>",
            ),
            (&["links"][..], "Usage: wikipedia links --title <TITLE>"),
        ] {
            let (stdout, stderr, status) = rendered(words);
            assert_eq!(status, 2, "{words:?}");
            assert!(stdout.is_empty(), "{words:?}: {stdout}");
            assert!(stderr.contains(usage), "{words:?}: {stderr}");
            if !words.is_empty() {
                assert!(stderr.starts_with("error: "), "{words:?}: {stderr}");
            }
        }
    }

    #[test]
    fn an_integer_outside_its_ceiling_is_a_usage_error_naming_the_flag() {
        for words in [
            &["search", "Ada", "--limit", "0"][..],
            &["search", "Ada", "--limit", "11"][..],
            &["page", "--title", "Ada", "--max-chars", "1201"][..],
            &["page", "--title", "Ada", "--max-chars", "many"][..],
            &["outline", "--title", "Ada", "--max-sections", "61"][..],
            &[
                "section",
                "--title",
                "Ada",
                "--section-index",
                "1",
                "--max-chars",
                "8001",
            ][..],
            &["links", "--title", "Ada", "--limit", "21"][..],
        ] {
            let (flag, value) = (words[words.len() - 2], words[words.len() - 1]);
            let (stdout, stderr, status) = rendered(words);
            assert_eq!(status, 2, "{words:?}");
            assert!(stdout.is_empty(), "{words:?}: {stdout}");
            assert!(
                stderr.starts_with(&format!("error: invalid value '{value}' for '{flag} <N>'")),
                "{words:?}: {stderr}"
            );
        }
    }

    /// The camelCase the shell's retired rewrite produced, the snake_case field names, and the
    /// flag-shaped query are refused by name rather than read as something else.
    #[test]
    fn retired_spellings_are_refused_by_name() {
        for (words, flag) in [
            (
                &["page", "--title", "Ada", "--maxChars", "1200"][..],
                "--maxChars",
            ),
            (
                &["page", "--title", "Ada", "--max_chars", "1200"][..],
                "--max_chars",
            ),
            (
                &["section", "--title", "Ada", "--sectionIndex", "1"][..],
                "--sectionIndex",
            ),
            (&["search", "--query", "Ada"][..], "--query"),
        ] {
            let (_, stderr, status) = rendered(words);
            assert_eq!(status, 2, "{words:?}");
            assert!(
                stderr.starts_with(&format!("error: unexpected argument '{flag}' found")),
                "{words:?}: {stderr}"
            );
        }
    }

    /// Each flag is the kebab-case spelling of one snake_case input field, and every field is sent
    /// with its default filled in.
    #[test]
    fn flags_propose_the_snake_case_input_fields() {
        let cases: [(&[&str], &str, Value); 9] = [
            (
                &["search", "Ada", "Lovelace"],
                SEARCH,
                json!({"query": "Ada Lovelace", "language": "en", "limit": 5}),
            ),
            (
                &[
                    "search",
                    "Ada Lovelace",
                    "--language",
                    "de",
                    "--limit",
                    "10",
                    "--cursor",
                    "eyJ2IjoxfQ",
                ],
                SEARCH,
                json!({"query": "Ada Lovelace", "language": "de", "limit": 10, "cursor": "eyJ2IjoxfQ"}),
            ),
            (
                &["page", "--title", "Ada Lovelace", "--max-chars", "1200"],
                PAGE,
                json!({"title": "Ada Lovelace", "language": "en", "max_chars": 1200}),
            ),
            (
                &["page", "--title", "-ism"],
                PAGE,
                json!({"title": "-ism", "language": "en", "max_chars": 900}),
            ),
            (
                &[
                    "outline",
                    "--title",
                    "Ada Lovelace",
                    "--language",
                    "fr",
                    "--max-sections",
                    "60",
                ],
                OUTLINE,
                json!({"title": "Ada Lovelace", "language": "fr", "max_sections": 60}),
            ),
            (
                &[
                    "section",
                    "--title",
                    "Ada Lovelace",
                    "--section-index",
                    "1",
                    "--max-chars",
                    "8000",
                ],
                SECTION,
                json!({"title": "Ada Lovelace", "section_index": "1", "language": "en", "max_chars": 8000}),
            ),
            (
                &[
                    "section",
                    "--title",
                    "Ada Lovelace",
                    "--section-index",
                    "T-1",
                ],
                SECTION,
                json!({"title": "Ada Lovelace", "section_index": "T-1", "language": "en", "max_chars": 3000}),
            ),
            (
                &["links", "--title", "Ada Lovelace"],
                LINKS,
                json!({"title": "Ada Lovelace", "language": "en", "limit": 20}),
            ),
            (
                &[
                    "links",
                    "--title",
                    "Ada Lovelace",
                    "--limit",
                    "2",
                    "--cursor",
                    "eyJ2IjoxfQ",
                ],
                LINKS,
                json!({"title": "Ada Lovelace", "language": "en", "limit": 2, "cursor": "eyJ2IjoxfQ"}),
            ),
        ];
        for (words, capability, input) in cases {
            let invocation = proposal(words);
            assert_eq!(invocation.capability.as_str(), capability, "{words:?}");
            assert_eq!(invocation.input, input, "{words:?}");
        }
    }

    /// The five verbs propose the five capabilities the manifest declares, in order. Without this,
    /// a renamed capability would reach a model at runtime as an authorization denial.
    #[test]
    fn every_dispatch_target_is_declared_in_the_manifest() {
        let manifest = MediaWiki::manifest();
        let declared: Vec<&str> = manifest
            .capabilities
            .iter()
            .map(|capability| capability.id.as_str())
            .collect();
        let proposed: Vec<String> = [
            &["search", "Ada"][..],
            &["page", "--title", "Ada"][..],
            &["outline", "--title", "Ada"][..],
            &["section", "--title", "Ada", "--section-index", "1"][..],
            &["links", "--title", "Ada"][..],
        ]
        .into_iter()
        .map(|words| proposal(words).capability.to_string())
        .collect();
        assert_eq!(proposed, declared);
    }

    #[test]
    fn search_round_trips_through_invoke() {
        let (output, uris) = round_trip(
            &["search", "Ada", "Lovelace", "--limit", "2"],
            &[fixture("search-page-1")],
        );
        assert!(
            uris[0].contains("&srlimit=2&srsearch=Ada+Lovelace"),
            "{}",
            uris[0]
        );
        assert_eq!(output["results"][0]["title"], "Ada Lovelace");
    }

    /// The production failure: this argv reached `invoke` as `maxChars` and was refused.
    #[test]
    fn page_round_trips_through_invoke() {
        let (output, _) = round_trip(
            &["page", "--title", "NYC", "--max-chars", "1200"],
            &[fixture("page-redirect")],
        );
        assert_eq!(output["requested_title"], "NYC");
        assert_eq!(output["title"], "New York City");
    }

    /// The flow the help page teaches: an index copied out of outline's output, unchanged, into
    /// section, which reads it from the revision it resolves.
    #[test]
    fn outline_then_section_round_trips_with_a_copied_index() {
        let (outline, _) = round_trip(
            &["outline", "--title", "Ada Lovelace", "--max-sections", "2"],
            &[fixture("outline-resolve"), fixture("outline")],
        );
        let index = outline["sections"][0]["index"]
            .as_str()
            .expect("an index string");
        let (section, uris) = round_trip(
            &[
                "section",
                "--title",
                "Ada Lovelace",
                "--section-index",
                index,
            ],
            &[
                fixture("outline-resolve"),
                fixture("outline"),
                fixture("section"),
            ],
        );
        assert!(uris[2].contains(&format!("section={index}")), "{}", uris[2]);
        assert_eq!(section["index"], index);
        assert_eq!(section["heading"], "Biography");
    }

    /// A cursor copied out of one links output into the next argv reaches `invoke` unchanged and
    /// still matches its request, defaults included.
    #[test]
    fn links_round_trip_through_invoke_with_a_copied_cursor() {
        let (first, _) = round_trip(
            &["links", "--title", "Ada Lovelace", "--limit", "2"],
            &[fixture("links-page-1")],
        );
        let cursor = first["next_cursor"].as_str().expect("a cursor");
        let (second, uris) = round_trip(
            &[
                "links",
                "--title",
                "Ada Lovelace",
                "--limit",
                "2",
                "--cursor",
                cursor,
            ],
            &[LINKS_PAGE_2],
        );
        assert!(
            uris[0].contains("plcontinue=974%7C0%7CA_Prince_of_Lovers"),
            "{}",
            uris[0]
        );
        assert_eq!(second["links"][0]["title"], "Analytical Engine");
    }
}
