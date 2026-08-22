use std::collections::HashSet;

use dekopon_provider_http::{Header, HttpError, Request, Response, method};
use dekopon_provider_sdk::ProviderError;
use serde::{Deserialize, Serialize};

use crate::{
    budget,
    cursor::{self, Continuation, ToolKind},
    error::{self, Operation},
    html_text,
    input::{self, LinksInput, OutlineInput, PageInput, SearchInput, SectionInput},
};

const USER_AGENT: &str = "dekopon-provider-mediawiki/0.1.0 (+https://github.com/dekopon-agents/dekopon-provider-mediawiki)";
const MAX_UPSTREAM_BODY_BYTES: usize = 1024 * 1024;
const MAX_SNIPPET_CHARACTERS: usize = 280;
const MAX_SNIPPET_BYTES: usize = 320;
const MAX_DESCRIPTION_CHARACTERS: usize = 512;
const MAX_DESCRIPTION_BYTES: usize = 1_024;
const MAX_HEADING_CHARACTERS: usize = 256;
const MAX_HEADING_BYTES: usize = 512;
const MAX_ANCHOR_BYTES: usize = 768;
const MAX_REDIRECTS: usize = 16;
const MAX_TIMESTAMP_BYTES: usize = 64;

pub(crate) type Send<'a> = &'a mut dyn FnMut(Request) -> Result<Response, HttpError>;

pub(crate) fn search(
    input: SearchInput,
    send: Send<'_>,
) -> Result<serde_json::Value, ProviderError> {
    let (continuation, depth, had_cursor) = decode_cursor(
        input.cursor.as_deref(),
        ToolKind::Search,
        &input.language,
        &input.query,
        input.limit,
    )?;
    let mut request = ApiRequest::new(&input.language, "query");
    request
        .pair("list", "search")
        .pair("srnamespace", "0")
        .pair("srprop", "snippet|wordcount|timestamp")
        .pair("srlimit", input.limit.to_string())
        .pair("srsearch", &input.query);
    append_continuation(&mut request, continuation.as_ref());

    let body = execute(send, request.finish()?, Operation::Search)?;
    let response: SearchResponse = decode(&body, Operation::Search)?;
    check_api_error(response.error.as_ref(), Operation::Search, had_cursor)?;
    let query = response.query.ok_or_else(error::upstream_error)?;
    if query.search.len() > input.limit {
        return Err(error::upstream_error());
    }

    let mut results = Vec::with_capacity(query.search.len());
    for result in query.search {
        if result.ns != 0 || result.pageid == 0 {
            return Err(error::upstream_error());
        }
        let title = upstream_title(result.title, Operation::Search)?;
        if result.timestamp.is_empty()
            || result.timestamp.len() > MAX_TIMESTAMP_BYTES
            || result.timestamp.chars().any(char::is_control)
        {
            return Err(error::upstream_error());
        }
        let rendered = render_html(&result.snippet)?;
        let snippet = budget::compact_plain_text(&rendered.text);
        let (snippet, _) =
            budget::truncate_text(&snippet, MAX_SNIPPET_CHARACTERS, MAX_SNIPPET_BYTES);
        results.push(SearchResult {
            page_id: result.pageid,
            title,
            snippet,
            word_count: result.wordcount,
            modified: result.timestamp,
        });
    }

    let next_page = cursor::encode_next(
        response.continuation,
        ToolKind::Search,
        &input.language,
        &input.query,
        input.limit,
        depth,
    )?;
    let mut output = SearchOutput {
        results,
        total_hits: query.searchinfo.totalhits,
        next_cursor: next_page.cursor,
        pagination_capped: next_page.pagination_capped,
    };
    shrink_search_to_fit(&mut output)?;
    budget::finish(&output)
}

pub(crate) fn page(input: PageInput, send: Send<'_>) -> Result<serde_json::Value, ProviderError> {
    let mut request = ApiRequest::new(&input.language, "query");
    request
        .pair("prop", "extracts|description|info|pageprops")
        .pair("redirects", "1")
        .pair("converttitles", "1")
        .pair("exintro", "1")
        .pair("explaintext", "1")
        .pair("exlimit", "1")
        .pair("titles", &input.title);

    let body = execute(send, request.finish()?, Operation::Page)?;
    let response: PageResponse = decode(&body, Operation::Page)?;
    check_api_error(response.error.as_ref(), Operation::Page, false)?;
    let query = response.query.ok_or_else(error::upstream_error)?;
    let page = one_page(query.pages, Operation::Page)?;
    if page.missing || page.ns != 0 {
        return Err(error::not_found());
    }
    let page_id = positive(page.pageid, Operation::Page)?;
    let revision_id = positive(page.lastrevid, Operation::Page)?;
    let title = upstream_title(page.title, Operation::Page)?;
    let extract = page.extract.ok_or_else(error::upstream_error)?;

    let mut truncated = false;
    let lead = budget::compact_plain_text(&extract);
    let (lead, lead_truncated) =
        budget::truncate_text(&lead, input.max_chars, input.max_chars.saturating_mul(4));
    truncated |= lead_truncated;

    let description = match page.description {
        Some(description) => {
            let description = budget::compact_plain_text(&description);
            let (description, was_truncated) = budget::truncate_text(
                &description,
                MAX_DESCRIPTION_CHARACTERS,
                MAX_DESCRIPTION_BYTES,
            );
            truncated |= was_truncated;
            (!description.is_empty()).then_some(description)
        }
        None => None,
    };

    let (redirects, redirects_truncated) = collect_redirects(
        query.normalized,
        query.converted,
        query.redirects,
        Operation::Page,
    )?;
    truncated |= redirects_truncated;
    let wikidata_id = validate_wikidata(page.pageprops.wikibase_item)?;
    let is_disambiguation = page.pageprops.disambiguation.is_some();
    let url = page_url(&input.language, &title, None);
    let mut output = PageOutput {
        requested_title: input.title,
        title,
        page_id,
        revision_id,
        description,
        lead,
        url,
        wikidata_id,
        is_disambiguation,
        redirects,
        truncated,
    };
    shrink_page_to_fit(&mut output)?;
    budget::finish(&output)
}

pub(crate) fn outline(
    input: OutlineInput,
    send: Send<'_>,
) -> Result<serde_json::Value, ProviderError> {
    let parsed = fetch_outline(send, &input.language, &input.title)?;
    let mut truncated = parsed.sections.len() > input.max_sections;
    let mut sections = Vec::with_capacity(parsed.sections.len().min(input.max_sections));
    for section in parsed.sections.into_iter().take(input.max_sections) {
        let (section, section_truncated) = project_outline_section(section, Operation::Outline)?;
        truncated |= section_truncated;
        sections.push(section);
    }
    let mut output = OutlineOutput {
        title: parsed.title,
        page_id: parsed.page_id,
        revision_id: parsed.revision_id,
        sections,
        truncated,
    };
    while !budget::projected_fits(&output) && !output.sections.is_empty() {
        output.sections.pop();
        output.truncated = true;
    }
    budget::finish(&output)
}

pub(crate) fn section(
    input: SectionInput,
    send: Send<'_>,
) -> Result<serde_json::Value, ProviderError> {
    let outline = fetch_outline(send, &input.language, &input.title)?;
    let selected = outline
        .sections
        .into_iter()
        .find(|section| section.index == input.section_index)
        .ok_or_else(error::no_such_section)?;
    let (heading, heading_truncated) = decode_heading(&selected.line, Operation::Section)?;
    validate_anchor(&selected.anchor, Operation::Section)?;

    let mut request = ApiRequest::new(&input.language, "parse");
    request
        .pair("oldid", outline.revision_id.to_string())
        .pair("section", &input.section_index)
        .pair("prop", "text|revid");
    let body = execute(send, request.finish()?, Operation::Section)?;
    let response: SectionResponse = decode(&body, Operation::Section)?;
    check_api_error(response.error.as_ref(), Operation::Section, false)?;
    let parsed = response.parse.ok_or_else(error::parse_failed)?;
    if parsed.pageid != outline.page_id
        || parsed.revid != outline.revision_id
        || parsed.title != outline.title
        || parsed.pageid == 0
        || parsed.revid == 0
    {
        return Err(error::parse_failed());
    }

    let rendered = render_html(&parsed.text)?;
    let text = html_text::remove_repeated_heading(rendered.text, &heading);
    let (text, text_truncated) =
        budget::truncate_text(&text, input.max_chars, input.max_chars.saturating_mul(4));
    let truncated = heading_truncated || rendered.truncated || text_truncated;
    let url = page_url(&input.language, &outline.title, Some(&selected.anchor));
    let mut output = SectionOutput {
        title: outline.title,
        page_id: outline.page_id,
        revision_id: outline.revision_id,
        index: input.section_index,
        heading,
        text,
        url,
        truncated,
    };
    shrink_section_to_fit(&mut output)?;
    budget::finish(&output)
}

pub(crate) fn links(input: LinksInput, send: Send<'_>) -> Result<serde_json::Value, ProviderError> {
    let (continuation, depth, had_cursor) = decode_cursor(
        input.cursor.as_deref(),
        ToolKind::Links,
        &input.language,
        &input.title,
        input.limit,
    )?;
    let mut request = ApiRequest::new(&input.language, "query");
    request
        .pair("prop", "links")
        .pair("plnamespace", "0")
        .pair("pllimit", input.limit.to_string())
        .pair("redirects", "1")
        .pair("converttitles", "1")
        .pair("titles", &input.title);
    append_continuation(&mut request, continuation.as_ref());

    let body = execute(send, request.finish()?, Operation::Links)?;
    let response: LinksResponse = decode(&body, Operation::Links)?;
    check_api_error(response.error.as_ref(), Operation::Links, had_cursor)?;
    let query = response.query.ok_or_else(error::upstream_error)?;
    let page = one_page(query.pages, Operation::Links)?;
    if page.missing || page.ns != 0 {
        return Err(error::not_found());
    }
    let page_id = positive(page.pageid, Operation::Links)?;
    let title = upstream_title(page.title, Operation::Links)?;
    let raw_links = page.links.unwrap_or_default();
    if raw_links.len() > input.limit {
        return Err(error::upstream_error());
    }
    let mut links = Vec::with_capacity(raw_links.len());
    for link in raw_links {
        if link.ns != 0 {
            return Err(error::upstream_error());
        }
        links.push(Link {
            title: upstream_title(link.title, Operation::Links)?,
        });
    }
    let next_page = cursor::encode_next(
        response.continuation,
        ToolKind::Links,
        &input.language,
        &input.title,
        input.limit,
        depth,
    )?;
    budget::finish(&LinksOutput {
        title,
        page_id,
        links,
        next_cursor: next_page.cursor,
        pagination_capped: next_page.pagination_capped,
    })
}

fn fetch_outline(
    send: Send<'_>,
    language: &str,
    requested_title: &str,
) -> Result<ResolvedOutline, ProviderError> {
    // Resolve namespace and revision before parsing. This keeps outline/section inside the same
    // main-namespace boundary as search, page, and links and pins the following parse call.
    let mut resolve = ApiRequest::new(language, "query");
    resolve
        .pair("prop", "info")
        .pair("redirects", "1")
        .pair("converttitles", "1")
        .pair("titles", requested_title);
    let body = execute(send, resolve.finish()?, Operation::Outline)?;
    let response: ResolveResponse = decode(&body, Operation::Outline)?;
    check_api_error(response.error.as_ref(), Operation::Outline, false)?;
    let query = response.query.ok_or_else(error::parse_failed)?;
    let page = one_page(query.pages, Operation::Outline)?;
    if page.missing || page.ns != 0 {
        return Err(error::not_found());
    }
    let page_id = positive(page.pageid, Operation::Outline)?;
    let revision_id = positive(page.lastrevid, Operation::Outline)?;
    let title = upstream_title(page.title, Operation::Outline)?;

    let mut parse = ApiRequest::new(language, "parse");
    parse
        .pair("oldid", revision_id.to_string())
        .pair("prop", "sections|revid");
    let body = execute(send, parse.finish()?, Operation::Outline)?;
    let response: OutlineResponse = decode(&body, Operation::Outline)?;
    check_api_error(response.error.as_ref(), Operation::Outline, false)?;
    let parsed = response.parse.ok_or_else(error::parse_failed)?;
    if parsed.pageid != page_id || parsed.revid != revision_id || parsed.title != title {
        return Err(error::parse_failed());
    }
    Ok(ResolvedOutline {
        title,
        page_id,
        revision_id,
        sections: parsed.sections,
    })
}

fn decode_cursor(
    encoded: Option<&str>,
    tool: ToolKind,
    language: &str,
    subject: &str,
    limit: usize,
) -> Result<(Option<Continuation>, u8, bool), ProviderError> {
    match encoded {
        Some(encoded) => {
            let state = cursor::decode(encoded, tool, language, subject, limit)?;
            Ok((Some(state.continuation), state.depth, true))
        }
        None => Ok((None, 0, false)),
    }
}

fn append_continuation(request: &mut ApiRequest, continuation: Option<&Continuation>) {
    let Some(continuation) = continuation else {
        return;
    };
    if let Some(value) = &continuation.generic {
        request.pair("continue", value);
    }
    if let Some(value) = continuation.sroffset {
        request.pair("sroffset", value.to_string());
    }
    if let Some(value) = &continuation.srcontinue {
        request.pair("srcontinue", value);
    }
    if let Some(value) = &continuation.plcontinue {
        request.pair("plcontinue", value);
    }
}

struct ApiRequest {
    language: String,
    parameters: Vec<(String, String)>,
}

impl ApiRequest {
    fn new(language: &str, action: &str) -> Self {
        Self {
            language: language.to_owned(),
            parameters: vec![
                ("action".to_owned(), action.to_owned()),
                ("format".to_owned(), "json".to_owned()),
                ("formatversion".to_owned(), "2".to_owned()),
                ("errorformat".to_owned(), "plaintext".to_owned()),
                ("maxlag".to_owned(), "5".to_owned()),
            ],
        }
    }

    fn pair(&mut self, name: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.parameters.push((name.into(), value.into()));
        self
    }

    fn finish(self) -> Result<Request, ProviderError> {
        // Defense in depth: this is already validated before construction.
        input::validate_language(&self.language)?;
        let mut serializer = form_urlencoded::Serializer::new(String::new());
        for (name, value) in self.parameters {
            serializer.append_pair(&name, &value);
        }
        let query = serializer.finish();
        let uri = format!(
            "https://{}.wikipedia.org/w/api.php?{}",
            self.language, query
        );
        Ok(Request::new(method::GET, uri)
            .map_err(|_| error::invalid_request())?
            .with_header(
                Header::text("accept", "application/json").map_err(|_| error::invalid_request())?,
            )
            .with_header(
                Header::text("user-agent", USER_AGENT).map_err(|_| error::invalid_request())?,
            ))
    }
}

fn execute(
    send: Send<'_>,
    request: Request,
    _operation: Operation,
) -> Result<Vec<u8>, ProviderError> {
    let response = send(request).map_err(|failure| error::transport(&failure))?;
    if response.status != 200 {
        return Err(error::status(response.status));
    }
    if response.body.len() > MAX_UPSTREAM_BODY_BYTES {
        return Err(error::response_too_large());
    }
    Ok(response.body)
}

fn decode<T: for<'de> Deserialize<'de>>(
    body: &[u8],
    operation: Operation,
) -> Result<T, ProviderError> {
    serde_json::from_slice(body).map_err(|_| error::malformed(operation))
}

fn render_html(fragment: &str) -> Result<html_text::PlainText, ProviderError> {
    html_text::to_plain_text(fragment).map_err(|_| error::response_too_large())
}

fn check_api_error(
    failure: Option<&ApiFailure>,
    operation: Operation,
    had_cursor: bool,
) -> Result<(), ProviderError> {
    if let Some(failure) = failure {
        if failure.code.is_empty()
            || failure.code.len() > 128
            || failure.code.chars().any(char::is_control)
        {
            return Err(error::malformed(operation));
        }
        return Err(error::api(&failure.code, operation, had_cursor));
    }
    Ok(())
}

fn one_page<T>(mut pages: Vec<T>, operation: Operation) -> Result<T, ProviderError> {
    if pages.len() != 1 {
        return Err(error::malformed(operation));
    }
    pages.pop().ok_or_else(|| error::malformed(operation))
}

fn positive(value: Option<u64>, operation: Operation) -> Result<u64, ProviderError> {
    value
        .filter(|value| *value > 0)
        .ok_or_else(|| error::malformed(operation))
}

fn upstream_title(title: String, operation: Operation) -> Result<String, ProviderError> {
    if !input::valid_title(&title) {
        return Err(error::malformed(operation));
    }
    Ok(title)
}

fn validate_wikidata(value: Option<String>) -> Result<Option<String>, ProviderError> {
    if let Some(value) = &value
        && (value.len() < 2
            || value.len() > 32
            || !value.starts_with('Q')
            || !value[1..].bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(error::upstream_error());
    }
    Ok(value)
}

fn collect_redirects(
    normalized: Vec<TitleChange>,
    converted: Vec<TitleChange>,
    redirects: Vec<TitleChange>,
    operation: Operation,
) -> Result<(Vec<Redirect>, bool), ProviderError> {
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    let mut truncated = false;
    for change in normalized.into_iter().chain(converted).chain(redirects) {
        let from = upstream_title(change.from, operation)?;
        let to = upstream_title(change.to, operation)?;
        if from == to || !seen.insert((from.clone(), to.clone())) {
            continue;
        }
        if output.len() == MAX_REDIRECTS {
            truncated = true;
            continue;
        }
        output.push(Redirect { from, to });
    }
    Ok((output, truncated))
}

fn validate_anchor(anchor: &str, operation: Operation) -> Result<(), ProviderError> {
    if anchor.is_empty() || anchor.len() > MAX_ANCHOR_BYTES || anchor.chars().any(char::is_control)
    {
        return Err(error::malformed(operation));
    }
    Ok(())
}

fn decode_heading(line: &str, operation: Operation) -> Result<(String, bool), ProviderError> {
    let rendered = render_html(line)?;
    let heading = budget::compact_plain_text(&rendered.text);
    if heading.is_empty()
        || heading.chars().count() > MAX_HEADING_CHARACTERS
        || heading.len() > MAX_HEADING_BYTES
    {
        return Err(error::malformed(operation));
    }
    Ok((heading, rendered.truncated))
}

fn project_outline_section(
    section: RawSection,
    operation: Operation,
) -> Result<(OutlineSection, bool), ProviderError> {
    if !valid_section_index(&section.index)
        || section.number.is_empty()
        || section.number.len() > 64
        || section.number.chars().any(char::is_control)
    {
        return Err(error::malformed(operation));
    }
    validate_anchor(&section.anchor, operation)?;
    let level = section
        .level
        .parse::<u8>()
        .ok()
        .filter(|level| (1..=6).contains(level))
        .ok_or_else(|| error::malformed(operation))?;
    let rendered = render_html(&section.line)?;
    let heading = budget::compact_plain_text(&rendered.text);
    if heading.is_empty() {
        return Err(error::malformed(operation));
    }
    let (title, title_truncated) =
        budget::truncate_text(&heading, MAX_HEADING_CHARACTERS, MAX_HEADING_BYTES);
    let truncated = rendered.truncated || title_truncated;
    Ok((
        OutlineSection {
            index: section.index,
            number: section.number,
            level,
            title,
            anchor: section.anchor,
        },
        truncated,
    ))
}

fn valid_section_index(index: &str) -> bool {
    !index.is_empty()
        && index.len() <= 32
        && !index
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
}

fn page_url(language: &str, title: &str, anchor: Option<&str>) -> String {
    let title = title.replace(' ', "_");
    let mut url = format!(
        "https://{language}.wikipedia.org/wiki/{}",
        percent_encode_component(&title)
    );
    if let Some(anchor) = anchor {
        url.push('#');
        url.push_str(&percent_encode_component(anchor));
    }
    url
}

fn percent_encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    encoded
}

fn shrink_search_to_fit(output: &mut SearchOutput) -> Result<(), ProviderError> {
    while !budget::projected_fits(output) {
        let serialized = budget::serialized_len(output)?;
        let Some(result) = output
            .results
            .iter_mut()
            .max_by_key(|result| result.snippet.len())
            .filter(|result| !result.snippet.is_empty())
        else {
            return Err(error::response_too_large());
        };
        let target = budget::next_text_target(result.snippet.len(), serialized);
        budget::truncate_bytes_in_place(&mut result.snippet, target);
    }
    Ok(())
}

fn shrink_page_to_fit(output: &mut PageOutput) -> Result<(), ProviderError> {
    while !budget::projected_fits(output) {
        // Preserve the compact answer before a long redirect/normalization chain.
        if output.redirects.pop().is_some() {
            output.truncated = true;
            continue;
        }
        if output.lead.is_empty() {
            return Err(error::response_too_large());
        }
        let serialized = budget::serialized_len(output)?;
        let target = budget::next_text_target(output.lead.len(), serialized);
        budget::truncate_bytes_in_place(&mut output.lead, target);
        output.truncated = true;
    }
    Ok(())
}

fn shrink_section_to_fit(output: &mut SectionOutput) -> Result<(), ProviderError> {
    while !budget::projected_fits(output) {
        if output.text.is_empty() {
            return Err(error::response_too_large());
        }
        let serialized = budget::serialized_len(output)?;
        let target = budget::next_text_target(output.text.len(), serialized);
        budget::truncate_bytes_in_place(&mut output.text, target);
        output.truncated = true;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ApiFailure {
    code: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    error: Option<ApiFailure>,
    #[serde(default)]
    query: Option<SearchQuery>,
    #[serde(rename = "continue", default)]
    continuation: Option<Continuation>,
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    searchinfo: SearchInfo,
    search: Vec<RawSearchResult>,
}

#[derive(Debug, Deserialize)]
struct SearchInfo {
    totalhits: u64,
}

#[derive(Debug, Deserialize)]
struct RawSearchResult {
    ns: i32,
    title: String,
    pageid: u64,
    wordcount: u64,
    snippet: String,
    timestamp: String,
}

#[derive(Debug, Deserialize)]
struct PageResponse {
    #[serde(default)]
    error: Option<ApiFailure>,
    #[serde(default)]
    query: Option<PageQuery>,
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    #[serde(default)]
    normalized: Vec<TitleChange>,
    #[serde(default)]
    converted: Vec<TitleChange>,
    #[serde(default)]
    redirects: Vec<TitleChange>,
    pages: Vec<RawPage>,
}

#[derive(Debug, Deserialize)]
struct TitleChange {
    from: String,
    to: String,
}

#[derive(Debug, Default, Deserialize)]
struct RawPageProps {
    #[serde(default)]
    wikibase_item: Option<String>,
    #[serde(default)]
    disambiguation: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RawPage {
    #[serde(default)]
    pageid: Option<u64>,
    ns: i32,
    title: String,
    #[serde(default)]
    missing: bool,
    #[serde(default)]
    lastrevid: Option<u64>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    extract: Option<String>,
    #[serde(default)]
    pageprops: RawPageProps,
}

#[derive(Debug, Deserialize)]
struct ResolveResponse {
    #[serde(default)]
    error: Option<ApiFailure>,
    #[serde(default)]
    query: Option<ResolveQuery>,
}

#[derive(Debug, Deserialize)]
struct ResolveQuery {
    pages: Vec<RawResolvedPage>,
}

#[derive(Debug, Deserialize)]
struct RawResolvedPage {
    #[serde(default)]
    pageid: Option<u64>,
    ns: i32,
    title: String,
    #[serde(default)]
    missing: bool,
    #[serde(default)]
    lastrevid: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OutlineResponse {
    #[serde(default)]
    error: Option<ApiFailure>,
    #[serde(default)]
    parse: Option<RawOutline>,
}

#[derive(Debug, Deserialize)]
struct RawOutline {
    title: String,
    pageid: u64,
    revid: u64,
    sections: Vec<RawSection>,
}

#[derive(Debug, Deserialize)]
struct RawSection {
    level: String,
    line: String,
    number: String,
    index: String,
    anchor: String,
}

struct ResolvedOutline {
    title: String,
    page_id: u64,
    revision_id: u64,
    sections: Vec<RawSection>,
}

#[derive(Debug, Deserialize)]
struct SectionResponse {
    #[serde(default)]
    error: Option<ApiFailure>,
    #[serde(default)]
    parse: Option<RawParsedSection>,
}

#[derive(Debug, Deserialize)]
struct RawParsedSection {
    title: String,
    pageid: u64,
    revid: u64,
    text: String,
}

#[derive(Debug, Deserialize)]
struct LinksResponse {
    #[serde(default)]
    error: Option<ApiFailure>,
    #[serde(default)]
    query: Option<LinksQuery>,
    #[serde(rename = "continue", default)]
    continuation: Option<Continuation>,
}

#[derive(Debug, Deserialize)]
struct LinksQuery {
    pages: Vec<RawLinksPage>,
}

#[derive(Debug, Deserialize)]
struct RawLinksPage {
    #[serde(default)]
    pageid: Option<u64>,
    ns: i32,
    title: String,
    #[serde(default)]
    missing: bool,
    #[serde(default)]
    links: Option<Vec<RawLink>>,
}

#[derive(Debug, Deserialize)]
struct RawLink {
    ns: i32,
    title: String,
}

#[derive(Debug, Serialize)]
struct SearchOutput {
    results: Vec<SearchResult>,
    total_hits: u64,
    next_cursor: Option<String>,
    pagination_capped: bool,
}

#[derive(Debug, Serialize)]
struct SearchResult {
    page_id: u64,
    title: String,
    snippet: String,
    word_count: u64,
    modified: String,
}

#[derive(Debug, Serialize)]
struct PageOutput {
    requested_title: String,
    title: String,
    page_id: u64,
    revision_id: u64,
    description: Option<String>,
    lead: String,
    url: String,
    wikidata_id: Option<String>,
    is_disambiguation: bool,
    redirects: Vec<Redirect>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct Redirect {
    from: String,
    to: String,
}

#[derive(Debug, Serialize)]
struct OutlineOutput {
    title: String,
    page_id: u64,
    revision_id: u64,
    sections: Vec<OutlineSection>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct OutlineSection {
    index: String,
    number: String,
    level: u8,
    title: String,
    anchor: String,
}

#[derive(Debug, Serialize)]
struct SectionOutput {
    title: String,
    page_id: u64,
    revision_id: u64,
    index: String,
    heading: String,
    text: String,
    url: String,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct LinksOutput {
    title: String,
    page_id: u64,
    links: Vec<Link>,
    next_cursor: Option<String>,
    pagination_capped: bool,
}

#[derive(Debug, Serialize)]
struct Link {
    title: String,
}

#[cfg(test)]
mod tests {
    use dekopon_provider_http::{HttpError, HttpErrorCode, Request, Response};
    use serde_json::{Value, json};

    use crate::{
        cursor::{Continuation, ToolKind},
        invoke_with,
        testutil::capability,
    };

    use super::{Link, LinksOutput, render_html};

    fn fixture(name: &str) -> Vec<u8> {
        let text = match name {
            "search-page-1" => include_str!("../tests/fixtures/search-page-1.json"),
            "search-empty" => include_str!("../tests/fixtures/search-empty.json"),
            "page-redirect" => include_str!("../tests/fixtures/page-redirect.json"),
            "page-normalized" => include_str!("../tests/fixtures/page-normalized.json"),
            "page-disambiguation" => {
                include_str!("../tests/fixtures/page-disambiguation.json")
            }
            "page-missing" => include_str!("../tests/fixtures/page-missing.json"),
            "outline-resolve" => include_str!("../tests/fixtures/outline-resolve.json"),
            "outline" => include_str!("../tests/fixtures/outline.json"),
            "section" => include_str!("../tests/fixtures/section.json"),
            "links-page-1" => include_str!("../tests/fixtures/links-page-1.json"),
            _ => panic!("unknown fixture"),
        };
        text.as_bytes().to_vec()
    }

    fn response(body: Vec<u8>) -> Result<Response, HttpError> {
        Ok(Response {
            status: 200,
            headers: Vec::new(),
            body,
        })
    }

    #[test]
    fn search_uses_exact_encoded_get_and_projects_a_cursor() {
        let output = invoke_with(
            &capability("wikipedia_search"),
            json!({"query": "Ada & café", "language": "en", "limit": 2}),
            |request| {
                assert_standard_request(&request, "en");
                assert_eq!(
                    request.uri,
                    "https://en.wikipedia.org/w/api.php?action=query&format=json&formatversion=2&errorformat=plaintext&maxlag=5&list=search&srnamespace=0&srprop=snippet%7Cwordcount%7Ctimestamp&srlimit=2&srsearch=Ada+%26+caf%C3%A9"
                );
                response(fixture("search-page-1"))
            },
        )
        .expect("search succeeds");
        assert_eq!(output["results"][0]["title"], "Ada Lovelace");
        assert_eq!(
            output["results"][0]["snippet"],
            "Ada Lovelace was a mathematician & writer."
        );
        let cursor = output["next_cursor"].as_str().expect("cursor").to_owned();
        let second = invoke_with(
            &capability("wikipedia_search"),
            json!({"query": "Ada & café", "language": "en", "limit": 2, "cursor": cursor}),
            |request| {
                assert!(request.uri.contains("continue=-%7C%7C"));
                assert!(request.uri.contains("sroffset=2"));
                response(fixture("search-empty"))
            },
        )
        .expect("second search page succeeds");
        assert!(second["next_cursor"].is_null());
        assert_eq!(second["pagination_capped"], false);
    }

    #[test]
    fn empty_search_is_a_success() {
        let output = invoke_with(
            &capability("wikipedia_search"),
            json!({"query": "nothing here"}),
            |_| response(fixture("search-empty")),
        )
        .expect("empty search succeeds");
        assert_eq!(
            output,
            json!({
                "results": [],
                "total_hits": 0,
                "next_cursor": null,
                "pagination_capped": false
            })
        );
    }

    #[test]
    fn page_preserves_requested_and_canonical_identity() {
        let output = invoke_with(
            &capability("wikipedia_page"),
            json!({"title": "NYC", "max_chars": 24}),
            |_| response(fixture("page-redirect")),
        )
        .expect("redirecting page succeeds");
        assert_eq!(output["requested_title"], "NYC");
        assert_eq!(output["title"], "New York City");
        assert_eq!(output["url"], "https://en.wikipedia.org/wiki/New_York_City");
        assert_eq!(
            output["redirects"][0],
            json!({"from": "NYC", "to": "New York City"})
        );
        assert_eq!(output["wikidata_id"], "Q60");
        assert_eq!(output["truncated"], true);
    }

    #[test]
    fn normalization_and_redirects_are_reported_in_api_order() {
        let output = invoke_with(
            &capability("wikipedia_page"),
            json!({"title": "ada_lovelace"}),
            |_| response(fixture("page-normalized")),
        )
        .expect("normalized redirect succeeds");
        assert_eq!(output["requested_title"], "ada_lovelace");
        assert_eq!(output["title"], "Ada Lovelace");
        assert_eq!(
            output["redirects"],
            json!([
                {"from": "ada_lovelace", "to": "Ada lovelace"},
                {"from": "Ada lovelace", "to": "Ada Lovelace"}
            ])
        );
    }

    #[test]
    fn disambiguation_is_flagged_and_missing_is_not_guessed() {
        let output = invoke_with(
            &capability("wikipedia_page"),
            json!({"title": "Mercury"}),
            |_| response(fixture("page-disambiguation")),
        )
        .expect("disambiguation is a success");
        assert_eq!(output["is_disambiguation"], true);

        let error = invoke_with(
            &capability("wikipedia_page"),
            json!({"title": "Definitely Missing"}),
            |_| response(fixture("page-missing")),
        )
        .expect_err("missing page fails");
        assert_eq!(error.code(), "not_found");
    }

    #[test]
    fn outline_resolves_main_namespace_then_parses_the_pinned_revision() {
        let mut calls = 0;
        let output = invoke_with(
            &capability("wikipedia_outline"),
            json!({"title": "Ada Lovelace", "max_sections": 2}),
            |request| {
                calls += 1;
                match calls {
                    1 => {
                        assert!(request.uri.contains("action=query"));
                        assert!(request.uri.contains("prop=info"));
                        assert!(request.uri.contains("titles=Ada+Lovelace"));
                        response(fixture("outline-resolve"))
                    }
                    2 => {
                        assert!(request.uri.contains("action=parse"));
                        assert!(request.uri.contains("oldid=1370153024"));
                        assert!(request.uri.contains("prop=sections%7Crevid"));
                        response(fixture("outline"))
                    }
                    _ => panic!("unexpected request"),
                }
            },
        )
        .expect("outline succeeds");
        assert_eq!(calls, 2);
        assert_eq!(output["sections"].as_array().expect("array").len(), 2);
        assert_eq!(output["sections"][0]["index"], "1");
        assert_eq!(output["truncated"], true);
    }

    #[test]
    fn outline_rejects_non_main_namespace_and_missing_pages_before_parse() {
        for (page, expected) in [
            (
                json!({
                    "batchcomplete": true,
                    "query": {"pages": [{
                        "pageid": 123, "ns": 2, "title": "User:Example", "lastrevid": 456
                    }]}
                }),
                "not_found",
            ),
            (
                json!({"error": {"code": "missingtitle", "info": "not found"}}),
                "not_found",
            ),
        ] {
            let mut calls = 0;
            let error = invoke_with(
                &capability("wikipedia_outline"),
                json!({"title": "User:Example"}),
                |_| {
                    calls += 1;
                    response(serde_json::to_vec(&page).expect("fixture serializes"))
                },
            )
            .expect_err("non-main or missing page fails");
            assert_eq!(calls, 1);
            assert_eq!(error.code(), expected);
        }

        let error = invoke_with(
            &capability("wikipedia_section"),
            json!({"title": "Deleted Page", "section_index": "1"}),
            |_| response(br#"{"error":{"code":"missingtitle"}}"#.to_vec()),
        )
        .expect_err("missing section page fails during resolution");
        assert_eq!(error.code(), "not_found");
    }

    #[test]
    fn section_resolves_then_fetches_the_revision_pinned_index() {
        let mut call = 0;
        let output = invoke_with(
            &capability("wikipedia_section"),
            json!({"title": "Ada Lovelace", "section_index": "1", "max_chars": 500}),
            |request| {
                assert_standard_request(&request, "en");
                call += 1;
                match call {
                    1 => {
                        assert!(request.uri.contains("action=query"));
                        assert!(request.uri.contains("prop=info"));
                        response(fixture("outline-resolve"))
                    }
                    2 => {
                        assert!(request.uri.contains("action=parse"));
                        assert!(request.uri.contains("oldid=1370153024"));
                        assert!(request.uri.contains("prop=sections%7Crevid"));
                        response(fixture("outline"))
                    }
                    3 => {
                        assert!(request.uri.contains("oldid=1370153024"));
                        assert!(request.uri.contains("section=1"));
                        assert!(request.uri.contains("prop=text%7Crevid"));
                        response(fixture("section"))
                    }
                    _ => panic!("unexpected request"),
                }
            },
        )
        .expect("section succeeds");
        assert_eq!(call, 3);
        assert_eq!(output["heading"], "Biography");
        assert!(
            !output["text"]
                .as_str()
                .expect("text")
                .starts_with("Biography")
        );
        assert!(output["text"].as_str().expect("text").contains("Childhood"));
        assert_eq!(output["revision_id"], 1370153024_u64);
    }

    #[test]
    fn missing_section_stops_after_the_outline_and_guides_reselection() {
        let mut calls = 0;
        let error = invoke_with(
            &capability("wikipedia_section"),
            json!({"title": "Ada Lovelace", "section_index": "999"}),
            |_| {
                calls += 1;
                match calls {
                    1 => response(fixture("outline-resolve")),
                    2 => response(fixture("outline")),
                    _ => panic!("unexpected request"),
                }
            },
        )
        .expect_err("unknown outline index fails");
        assert_eq!(calls, 2);
        assert_eq!(error.code(), "no_such_section");
        assert!(error.message().contains("wikipedia_outline"));
    }

    #[test]
    fn links_replay_both_continuation_fields() {
        let first = invoke_with(
            &capability("wikipedia_links"),
            json!({"title": "Ada Lovelace", "limit": 2}),
            |_| response(fixture("links-page-1")),
        )
        .expect("first page succeeds");
        let cursor = first["next_cursor"].as_str().expect("cursor").to_owned();
        assert_eq!(first["links"].as_array().expect("links").len(), 2);
        assert_eq!(first["pagination_capped"], false);

        let _second = invoke_with(
            &capability("wikipedia_links"),
            json!({"title": "Ada Lovelace", "limit": 2, "cursor": cursor}),
            |request| {
                assert!(request.uri.contains("continue=%7C%7C"));
                assert!(
                    request
                        .uri
                        .contains("plcontinue=974%7C0%7CA_Prince_of_Lovers")
                );
                response(
                    serde_json::to_vec(&json!({
                        "batchcomplete": true,
                        "query": {"pages": [{
                            "pageid": 974, "ns": 0, "title": "Ada Lovelace",
                            "links": [{"ns": 0, "title": "Analytical Engine"}]
                        }]}
                    }))
                    .expect("fixture serializes"),
                )
            },
        )
        .expect("second page succeeds");
    }

    #[test]
    fn pagination_depth_cap_is_explicit_not_false_exhaustion() {
        let continuation = Continuation {
            generic: Some("-||".to_owned()),
            sroffset: Some(2),
            ..Continuation::default()
        };
        let mut cursor = None;
        for depth in 0..10 {
            cursor = crate::cursor::encode_next(
                Some(continuation.clone()),
                ToolKind::Search,
                "en",
                "Ada & café",
                2,
                depth,
            )
            .expect("cursor encodes")
            .cursor;
        }
        let output = invoke_with(
            &capability("wikipedia_search"),
            json!({
                "query": "Ada & café",
                "language": "en",
                "limit": 2,
                "cursor": cursor.expect("depth-ten cursor")
            }),
            |_| response(fixture("search-page-1")),
        )
        .expect("capped page succeeds");
        assert!(output["next_cursor"].is_null());
        assert_eq!(output["pagination_capped"], true);
    }

    #[test]
    fn twenty_links_fit_the_projection_with_worst_case_json_escaping_and_cursor() {
        let escaped_title = "\\".repeat(255);
        let output = LinksOutput {
            title: escaped_title.clone(),
            page_id: u64::MAX,
            links: (0..20)
                .map(|index| Link {
                    title: format!("{index:02}{}", "\\".repeat(253)),
                })
                .collect(),
            next_cursor: Some("x".repeat(2_048)),
            pagination_capped: false,
        };
        let length = crate::budget::serialized_len(&output).expect("projection serializes");
        assert_eq!(length, 13_074);
        let value = crate::budget::finish(&output).expect("worst-case links fit the SDK envelope");
        let envelope = dekopon_provider_sdk::ComponentResponse::Succeeded { output: value };
        assert_eq!(
            crate::budget::serialized_len(&envelope).expect("envelope serializes"),
            13_107
        );
    }

    #[test]
    fn excessive_html_depth_maps_to_a_structured_provider_error() {
        let fragment = format!("{}text{}", "<div>".repeat(300), "</div>".repeat(300));
        let error = render_html(&fragment).expect_err("deep DOM is rejected");
        assert_eq!(error.code(), "response_too_large");
    }

    #[test]
    fn errors_cover_transport_status_api_json_and_body_bounds() {
        let input = json!({"title": "Ada Lovelace"});
        let transport = invoke_with(&capability("wikipedia_page"), input.clone(), |_| {
            Err(HttpError {
                code: HttpErrorCode::Timeout,
                message: "secret transport detail".to_owned(),
            })
        })
        .expect_err("timeout fails");
        assert_eq!(transport.code(), "timeout");
        assert!(!transport.message().contains("secret"));

        for (status, expected) in [
            (302, "upstream_error"),
            (429, "rate_limited"),
            (503, "maxlag"),
            (500, "upstream_error"),
        ] {
            let error = invoke_with(&capability("wikipedia_page"), input.clone(), |_| {
                Ok(Response {
                    status,
                    headers: Vec::new(),
                    body: b"private body".to_vec(),
                })
            })
            .expect_err("status fails");
            assert_eq!(error.code(), expected);
            assert!(!error.message().contains("private"));
        }

        let malformed = invoke_with(&capability("wikipedia_outline"), input.clone(), |_| {
            response(b"not json".to_vec())
        })
        .expect_err("malformed JSON fails");
        assert_eq!(malformed.code(), "parse_failed");

        let maxlag = invoke_with(&capability("wikipedia_page"), input.clone(), |_| {
            response(br#"{"error":{"code":"maxlag","info":"private detail"}}"#.to_vec())
        })
        .expect_err("API maxlag fails");
        assert_eq!(maxlag.code(), "maxlag");

        let oversized = invoke_with(&capability("wikipedia_page"), input, |_| {
            response(vec![b'x'; 1024 * 1024 + 1])
        })
        .expect_err("oversized body fails");
        assert_eq!(oversized.code(), "response_too_large");
    }

    fn assert_standard_request(request: &Request, language: &str) {
        assert_eq!(request.method, "GET");
        assert!(request.body.is_empty());
        assert!(
            request
                .uri
                .starts_with(&format!("https://{language}.wikipedia.org/w/api.php?"))
        );
        assert!(!request.uri.contains('@'));
        assert!(request.headers.iter().any(|header| {
            header.name.eq_ignore_ascii_case("accept") && header.value == b"application/json"
        }));
        assert!(request.headers.iter().any(|header| {
            header.name.eq_ignore_ascii_case("user-agent")
                && header.value
                    == b"dekopon-provider-mediawiki/0.1.0 (+https://github.com/dekopon-agents/dekopon-provider-mediawiki)"
        }));
        assert!(
            !request
                .headers
                .iter()
                .any(|header| header.name.eq_ignore_ascii_case("authorization"))
        );
    }

    #[test]
    fn unknown_and_invalid_calls_make_no_http_request() {
        let error = invoke_with(&capability("wikipedia_random"), json!({}), |_| {
            panic!("unknown capability must not call HTTP")
        })
        .expect_err("unknown capability fails");
        assert_eq!(error.code(), "invalid_input");

        let error = invoke_with(
            &capability("wikipedia_search"),
            json!({"query": "insource:secret"}),
            |_| panic!("invalid query must not call HTTP"),
        )
        .expect_err("invalid query fails");
        assert_eq!(error.code(), "invalid_query");
    }

    #[test]
    fn worst_case_escaping_and_four_byte_text_stay_inside_the_sdk_envelope() {
        let extract = format!("{}{}", "😀".repeat(1_200), "\\\"".repeat(8_000));
        let body = serde_json::to_vec(&json!({
            "batchcomplete": true,
            "query": {"pages": [{
                "pageid": 1, "ns": 0, "title": "Escaping", "lastrevid": 2,
                "extract": extract, "pageprops": {}
            }]}
        }))
        .expect("fixture serializes");
        let output = invoke_with(
            &capability("wikipedia_page"),
            json!({"title": "Escaping", "max_chars": 1200}),
            |_| response(body.clone()),
        )
        .expect("bounded projection succeeds");
        let envelope = dekopon_provider_sdk::ComponentResponse::Succeeded { output };
        let bytes = serde_json::to_vec(&envelope).expect("envelope serializes");
        assert!(bytes.len() <= 16_384, "{}", bytes.len());
    }

    #[test]
    fn response_helpers_do_not_require_content_type() {
        let value: Value = invoke_with(
            &capability("wikipedia_search"),
            json!({"query": "nothing"}),
            |_| response(fixture("search-empty")),
        )
        .expect("valid JSON is sufficient");
        assert_eq!(value["total_hits"], 0);
    }
}
