use dekopon_provider_sdk::ProviderError;
use serde::Deserialize;
use serde_json::Value;

use crate::error;

pub(crate) const DEFAULT_LANGUAGE: &str = "en";
pub(crate) const MAX_TITLE_BYTES: usize = 255;
pub(crate) const MAX_CURSOR_BYTES: usize = 2 * 1024;

pub(crate) const DEFAULT_SEARCH_LIMIT: usize = 5;
pub(crate) const MAX_SEARCH_LIMIT: usize = 10;
pub(crate) const DEFAULT_PAGE_CHARS: usize = 900;
pub(crate) const MAX_PAGE_CHARS: usize = 1_200;
pub(crate) const DEFAULT_OUTLINE_SECTIONS: usize = 30;
pub(crate) const MAX_OUTLINE_SECTIONS: usize = 60;
pub(crate) const DEFAULT_SECTION_CHARS: usize = 3_000;
pub(crate) const MAX_SECTION_CHARS: usize = 8_000;
pub(crate) const DEFAULT_LINK_LIMIT: usize = 25;
pub(crate) const MAX_LINK_LIMIT: usize = 50;

/// Active, public Wikipedia edition host labels from Wikimedia SiteMatrix on 2026-08-22.
/// Source: `meta.wikimedia.org/w/api.php?action=sitematrix&format=json&formatversion=2`.
/// Closed, private, fishbowl, and special-project entries were excluded during authoring.
pub(crate) const ACTIVE_WIKIPEDIA_LANGUAGES: &[&str] = &[
    "ab",
    "ace",
    "ady",
    "af",
    "als",
    "alt",
    "am",
    "ami",
    "an",
    "ang",
    "ann",
    "anp",
    "ar",
    "arc",
    "ary",
    "arz",
    "as",
    "ast",
    "atj",
    "av",
    "avk",
    "awa",
    "ay",
    "az",
    "azb",
    "ba",
    "ban",
    "bar",
    "bat-smg",
    "bbc",
    "bcl",
    "bdr",
    "be",
    "be-tarask",
    "bew",
    "bg",
    "bh",
    "bi",
    "bjn",
    "blk",
    "bm",
    "bn",
    "bo",
    "bol",
    "bpy",
    "br",
    "bs",
    "btm",
    "bug",
    "bxr",
    "ca",
    "cbk-zam",
    "cdo",
    "ce",
    "ceb",
    "ch",
    "chr",
    "chy",
    "ckb",
    "co",
    "crh",
    "cs",
    "csb",
    "cu",
    "cv",
    "cy",
    "da",
    "dag",
    "de",
    "dga",
    "din",
    "diq",
    "dsb",
    "dtp",
    "dty",
    "dv",
    "dz",
    "ee",
    "el",
    "eml",
    "en",
    "eo",
    "es",
    "et",
    "eu",
    "ext",
    "fa",
    "fat",
    "ff",
    "fi",
    "fiu-vro",
    "fj",
    "fo",
    "fon",
    "fr",
    "frp",
    "frr",
    "fur",
    "fy",
    "ga",
    "gag",
    "gan",
    "gcr",
    "gd",
    "gl",
    "glk",
    "gn",
    "gom",
    "gor",
    "got",
    "gpe",
    "gu",
    "guc",
    "gur",
    "guw",
    "gv",
    "ha",
    "hak",
    "haw",
    "he",
    "hi",
    "hif",
    "hr",
    "hsb",
    "ht",
    "hu",
    "hy",
    "hyw",
    "ia",
    "iba",
    "id",
    "ie",
    "ig",
    "igl",
    "ik",
    "ilo",
    "inh",
    "io",
    "is",
    "isv",
    "it",
    "iu",
    "ja",
    "jam",
    "jbo",
    "jv",
    "ka",
    "kaa",
    "kab",
    "kai",
    "kaj",
    "kbd",
    "kbp",
    "kcg",
    "kg",
    "kge",
    "ki",
    "kk",
    "km",
    "kn",
    "knc",
    "ko",
    "koi",
    "krc",
    "ks",
    "ksh",
    "ku",
    "kus",
    "kv",
    "kw",
    "ky",
    "la",
    "lad",
    "lb",
    "lbe",
    "lez",
    "lfn",
    "lg",
    "li",
    "lij",
    "lld",
    "lmo",
    "ln",
    "lo",
    "lt",
    "ltg",
    "lv",
    "mad",
    "mag",
    "mai",
    "map-bms",
    "mdf",
    "mg",
    "mhr",
    "mi",
    "min",
    "mk",
    "ml",
    "mn",
    "mni",
    "mnw",
    "mos",
    "mr",
    "mrj",
    "ms",
    "mt",
    "mwl",
    "my",
    "myv",
    "mzn",
    "nah",
    "nap",
    "nds",
    "nds-nl",
    "ne",
    "new",
    "nia",
    "nl",
    "nn",
    "no",
    "nov",
    "nqo",
    "nr",
    "nrm",
    "nso",
    "nup",
    "nv",
    "ny",
    "oc",
    "olo",
    "om",
    "or",
    "os",
    "pa",
    "pag",
    "pam",
    "pap",
    "pcd",
    "pcm",
    "pdc",
    "pfl",
    "pi",
    "pl",
    "pms",
    "pnb",
    "pnt",
    "ppl",
    "ps",
    "pt",
    "pwn",
    "qu",
    "rki",
    "rm",
    "rmy",
    "rn",
    "ro",
    "roa-rup",
    "roa-tara",
    "rsk",
    "ru",
    "rue",
    "rw",
    "sa",
    "sah",
    "sat",
    "sc",
    "scn",
    "sco",
    "sd",
    "se",
    "sg",
    "sh",
    "shi",
    "shn",
    "si",
    "simple",
    "sk",
    "skr",
    "sl",
    "sm",
    "smn",
    "sn",
    "so",
    "sq",
    "sr",
    "srn",
    "ss",
    "st",
    "stq",
    "su",
    "sv",
    "sw",
    "syl",
    "szl",
    "szy",
    "ta",
    "tay",
    "tcy",
    "tdd",
    "te",
    "tet",
    "tg",
    "th",
    "ti",
    "tig",
    "tk",
    "tl",
    "tly",
    "tn",
    "to",
    "tok",
    "tpi",
    "tr",
    "trv",
    "ts",
    "tt",
    "tum",
    "tw",
    "ty",
    "tyv",
    "udm",
    "ug",
    "uk",
    "ur",
    "uz",
    "ve",
    "vec",
    "vep",
    "vi",
    "vls",
    "vo",
    "wa",
    "war",
    "wo",
    "wuu",
    "xal",
    "xh",
    "xmf",
    "yi",
    "yo",
    "za",
    "zea",
    "zgh",
    "zh",
    "zh-classical",
    "zh-min-nan",
    "zh-yue",
    "zu",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchInput {
    pub(crate) query: String,
    #[serde(default = "default_language")]
    pub(crate) language: String,
    #[serde(default = "default_search_limit")]
    pub(crate) limit: usize,
    #[serde(default)]
    pub(crate) cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PageInput {
    pub(crate) title: String,
    #[serde(default = "default_language")]
    pub(crate) language: String,
    #[serde(default = "default_page_chars")]
    pub(crate) max_chars: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OutlineInput {
    pub(crate) title: String,
    #[serde(default = "default_language")]
    pub(crate) language: String,
    #[serde(default = "default_outline_sections")]
    pub(crate) max_sections: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SectionInput {
    pub(crate) title: String,
    pub(crate) section_index: String,
    #[serde(default = "default_language")]
    pub(crate) language: String,
    #[serde(default = "default_section_chars")]
    pub(crate) max_chars: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LinksInput {
    pub(crate) title: String,
    #[serde(default = "default_language")]
    pub(crate) language: String,
    #[serde(default = "default_link_limit")]
    pub(crate) limit: usize,
    #[serde(default)]
    pub(crate) cursor: Option<String>,
}

pub(crate) fn parse_search(value: Value) -> Result<SearchInput, ProviderError> {
    let input: SearchInput = decode(value)?;
    validate_language(&input.language)?;
    validate_query(&input.query)?;
    validate_limit(input.limit, MAX_SEARCH_LIMIT)?;
    validate_cursor_shape(input.cursor.as_deref())?;
    Ok(input)
}

pub(crate) fn parse_page(value: Value) -> Result<PageInput, ProviderError> {
    let input: PageInput = decode(value)?;
    validate_language(&input.language)?;
    validate_title(&input.title)?;
    validate_limit(input.max_chars, MAX_PAGE_CHARS)?;
    Ok(input)
}

pub(crate) fn parse_outline(value: Value) -> Result<OutlineInput, ProviderError> {
    let input: OutlineInput = decode(value)?;
    validate_language(&input.language)?;
    validate_title(&input.title)?;
    validate_limit(input.max_sections, MAX_OUTLINE_SECTIONS)?;
    Ok(input)
}

pub(crate) fn parse_section(value: Value) -> Result<SectionInput, ProviderError> {
    let input: SectionInput = decode(value)?;
    validate_language(&input.language)?;
    validate_title(&input.title)?;
    validate_section_index(&input.section_index)?;
    validate_limit(input.max_chars, MAX_SECTION_CHARS)?;
    Ok(input)
}

pub(crate) fn parse_links(value: Value) -> Result<LinksInput, ProviderError> {
    let input: LinksInput = decode(value)?;
    validate_language(&input.language)?;
    validate_title(&input.title)?;
    validate_limit(input.limit, MAX_LINK_LIMIT)?;
    validate_cursor_shape(input.cursor.as_deref())?;
    Ok(input)
}

fn decode<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, ProviderError> {
    serde_json::from_value(value).map_err(|_| error::invalid_input())
}

pub(crate) fn validate_language(language: &str) -> Result<(), ProviderError> {
    if ACTIVE_WIKIPEDIA_LANGUAGES.binary_search(&language).is_err() {
        return Err(error::invalid_language());
    }
    Ok(())
}

pub(crate) fn validate_query(query: &str) -> Result<(), ProviderError> {
    let scalars = query.chars().count();
    if !(1..=256).contains(&scalars)
        || query.trim().is_empty()
        || query.chars().any(char::is_control)
        || query.to_ascii_lowercase().contains("insource:")
    {
        return Err(error::invalid_query());
    }
    Ok(())
}

pub(crate) fn validate_title(title: &str) -> Result<(), ProviderError> {
    if !valid_title(title) {
        return Err(error::invalid_title());
    }
    Ok(())
}

pub(crate) fn valid_title(title: &str) -> bool {
    !title.trim().is_empty()
        && title.len() <= MAX_TITLE_BYTES
        && !title.chars().any(char::is_control)
}

pub(crate) fn validate_section_index(index: &str) -> Result<(), ProviderError> {
    if index.is_empty()
        || index.len() > 32
        || index
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(error::invalid_section_index());
    }
    Ok(())
}

fn validate_limit(value: usize, maximum: usize) -> Result<(), ProviderError> {
    if !(1..=maximum).contains(&value) {
        return Err(error::invalid_input());
    }
    Ok(())
}

fn validate_cursor_shape(cursor: Option<&str>) -> Result<(), ProviderError> {
    if let Some(cursor) = cursor
        && (cursor.is_empty()
            || cursor.len() > MAX_CURSOR_BYTES
            || !cursor
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
    {
        return Err(error::invalid_cursor());
    }
    Ok(())
}

fn default_language() -> String {
    DEFAULT_LANGUAGE.to_owned()
}

fn default_search_limit() -> usize {
    DEFAULT_SEARCH_LIMIT
}

fn default_page_chars() -> usize {
    DEFAULT_PAGE_CHARS
}

fn default_outline_sections() -> usize {
    DEFAULT_OUTLINE_SECTIONS
}

fn default_section_chars() -> usize {
    DEFAULT_SECTION_CHARS
}

fn default_link_limit() -> usize {
    DEFAULT_LINK_LIMIT
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        ACTIVE_WIKIPEDIA_LANGUAGES, parse_links, parse_outline, parse_page, parse_search,
        parse_section,
    };

    #[test]
    fn language_snapshot_is_sorted_unique_and_defaults_to_english() {
        assert_eq!(ACTIVE_WIKIPEDIA_LANGUAGES.len(), 348);
        assert!(
            ACTIVE_WIKIPEDIA_LANGUAGES
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
        let input = parse_search(json!({"query": "Ada Lovelace"})).expect("valid input");
        assert_eq!(input.language, "en");
        assert_eq!(input.limit, 5);
    }

    #[test]
    fn strict_objects_and_language_allowlist_fail_before_http() {
        for input in [
            json!({"query": "Ada", "extra": true}),
            json!({"query": "Ada", "language": "EN"}),
            json!({"query": "Ada", "language": "meta"}),
            json!({"query": "Ada", "limit": 0}),
            json!({"query": "Ada", "limit": 11}),
        ] {
            assert!(parse_search(input).is_err());
        }
    }

    #[test]
    fn query_bounds_count_unicode_scalars_and_block_insource() {
        assert!(parse_search(json!({"query": "😀".repeat(256)})).is_ok());
        for query in [
            String::new(),
            "   ".to_owned(),
            "x\nnext".to_owned(),
            "INSOURCE:password".to_owned(),
            "😀".repeat(257),
        ] {
            let error = parse_search(json!({"query": query})).expect_err("query must fail");
            assert_eq!(error.code(), "invalid_query");
        }
    }

    #[test]
    fn titles_use_utf8_byte_bounds_and_indices_forbid_whitespace() {
        assert!(parse_page(json!({"title": "é".repeat(127)})).is_ok());
        let error = parse_page(json!({"title": "é".repeat(128)})).expect_err("256 bytes fails");
        assert_eq!(error.code(), "invalid_title");

        for index in ["", "1 2", "1\n2", &"x".repeat(33)] {
            let error = parse_section(json!({"title": "Ada Lovelace", "section_index": index}))
                .expect_err("index must fail");
            assert_eq!(error.code(), "invalid_input");
        }
    }

    #[test]
    fn every_numeric_ceiling_is_enforced_natively() {
        assert!(parse_page(json!({"title": "Ada", "max_chars": 1_200})).is_ok());
        assert!(parse_page(json!({"title": "Ada", "max_chars": 1_201})).is_err());
        assert!(parse_outline(json!({"title": "Ada", "max_sections": 60})).is_ok());
        assert!(parse_outline(json!({"title": "Ada", "max_sections": 61})).is_err());
        assert!(
            parse_section(json!({
                "title": "Ada",
                "section_index": "1",
                "max_chars": 8_000
            }))
            .is_ok()
        );
        assert!(
            parse_section(json!({
                "title": "Ada",
                "section_index": "1",
                "max_chars": 8_001
            }))
            .is_err()
        );
        assert!(parse_links(json!({"title": "Ada", "limit": 50})).is_ok());
        assert!(parse_links(json!({"title": "Ada", "limit": 51})).is_err());
    }

    #[test]
    fn cursors_are_lexically_bounded_before_decoding() {
        for cursor in ["=padding", "has space", &"x".repeat(2049)] {
            let error = parse_links(json!({"title": "Ada", "cursor": cursor}))
                .expect_err("cursor must fail");
            assert_eq!(error.code(), "invalid_cursor");
        }
    }
}
