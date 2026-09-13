use dekopon_provider_http::{HttpError, HttpErrorCode};
use dekopon_provider_sdk::ProviderError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Search,
    Page,
    Outline,
    Section,
    Links,
}

pub(crate) fn invalid_input() -> ProviderError {
    ProviderError::new(
        "invalid_input",
        "input does not match the closed capability contract; correct the fields and bounds",
    )
}

pub(crate) fn invalid_language() -> ProviderError {
    ProviderError::new(
        "invalid_language",
        "language must be a checked-in active Wikipedia edition code such as en or de",
    )
}

pub(crate) fn invalid_query() -> ProviderError {
    ProviderError::new(
        "invalid_query",
        "query must be 1-256 non-control characters and must not contain insource:",
    )
}

pub(crate) fn invalid_cursor() -> ProviderError {
    ProviderError::new(
        "invalid_cursor",
        "cursor is invalid or does not match this request; rerun the same command without --cursor",
    )
}

pub(crate) fn invalid_title() -> ProviderError {
    ProviderError::new(
        "invalid_title",
        "title must be nonblank, control-free UTF-8 of at most 255 bytes; copy one from `wikipedia search`",
    )
}

pub(crate) fn invalid_section_index() -> ProviderError {
    ProviderError::new(
        "invalid_input",
        "--section-index must be copied unchanged from `wikipedia outline`",
    )
}

pub(crate) fn not_found() -> ProviderError {
    ProviderError::new(
        "not_found",
        "Wikipedia has no matching main-namespace page; find the canonical title with `wikipedia search`",
    )
}

pub(crate) fn no_such_section() -> ProviderError {
    ProviderError::new(
        "no_such_section",
        "--section-index was not found; run `wikipedia outline` again and copy a current index",
    )
}

pub(crate) fn parse_failed() -> ProviderError {
    ProviderError::new(
        "parse_failed",
        "Wikipedia did not return a valid parsed page; rerun `wikipedia outline` or choose another page",
    )
}

pub(crate) fn rate_limited() -> ProviderError {
    ProviderError::new(
        "rate_limited",
        "Wikipedia rate-limited the request; wait before retrying",
    )
}

pub(crate) fn maxlag() -> ProviderError {
    ProviderError::new(
        "maxlag",
        "Wikipedia is busy and rejected the request due to maxlag; retry later",
    )
}

pub(crate) fn timeout() -> ProviderError {
    ProviderError::new(
        "timeout",
        "the broker-mediated Wikipedia request timed out; retry once or narrow the request",
    )
}

pub(crate) fn response_too_large() -> ProviderError {
    ProviderError::new(
        "response_too_large",
        "the Wikipedia response exceeded a safe size; narrow the query, text limit, or page size",
    )
}

pub(crate) fn upstream_error() -> ProviderError {
    ProviderError::new(
        "upstream_error",
        "Wikipedia returned an unusable response; retry later or choose another page",
    )
}

pub(crate) fn unknown_capability() -> ProviderError {
    ProviderError::new(
        "invalid_input",
        "unknown MediaWiki capability; run `wikipedia --help` for the five commands",
    )
}

pub(crate) fn invalid_request() -> ProviderError {
    ProviderError::new(
        "upstream_error",
        "the provider could not construct the fixed Wikipedia request",
    )
}

pub(crate) fn transport(error: &HttpError) -> ProviderError {
    match error.code {
        HttpErrorCode::Timeout => timeout(),
        HttpErrorCode::ResponseTooLarge => response_too_large(),
        _ => upstream_error(),
    }
}

pub(crate) fn status(status: u16) -> ProviderError {
    match status {
        429 => rate_limited(),
        503 => maxlag(),
        _ => upstream_error(),
    }
}

pub(crate) fn malformed(operation: Operation) -> ProviderError {
    match operation {
        Operation::Outline | Operation::Section => parse_failed(),
        Operation::Search | Operation::Page | Operation::Links => upstream_error(),
    }
}

pub(crate) fn api(code: &str, operation: Operation, had_cursor: bool) -> ProviderError {
    match code {
        "maxlag" => maxlag(),
        "ratelimited" | "cirrussearch-too-busy-error" => rate_limited(),
        "badcontinue" | "invalidcontinue" if had_cursor => invalid_cursor(),
        "missingtitle" | "invalidtitle" => match operation {
            Operation::Section => no_such_section(),
            Operation::Outline | Operation::Page | Operation::Links => not_found(),
            Operation::Search => upstream_error(),
        },
        "nosuchsection" if operation == Operation::Section => no_such_section(),
        "cirrussearch-query-too-long" | "search-invalid-query"
            if operation == Operation::Search =>
        {
            invalid_query()
        }
        _ => malformed(operation),
    }
}

#[cfg(test)]
mod tests {
    use dekopon_provider_http::{HttpError, HttpErrorCode};

    use super::{
        Operation, api, invalid_cursor, invalid_input, invalid_language, invalid_query,
        invalid_request, invalid_section_index, invalid_title, maxlag, no_such_section, not_found,
        parse_failed, rate_limited, response_too_large, status, timeout, transport,
        unknown_capability, upstream_error,
    };

    /// A model reaches this provider only through `wikipedia <verb>`, and a bare capability id is
    /// not a command it can run, so no message may send it to one.
    #[test]
    fn no_message_names_a_capability_id() {
        for error in [
            invalid_input(),
            invalid_language(),
            invalid_query(),
            invalid_cursor(),
            invalid_title(),
            invalid_section_index(),
            not_found(),
            no_such_section(),
            parse_failed(),
            rate_limited(),
            maxlag(),
            timeout(),
            response_too_large(),
            upstream_error(),
            unknown_capability(),
            invalid_request(),
        ] {
            assert!(
                !error.message().contains("wikipedia_"),
                "{}: {}",
                error.code(),
                error.message()
            );
        }
    }

    #[test]
    fn maps_transport_and_status_without_exposing_details() {
        let secret = "credential=secret internal route";
        for (code, expected) in [
            (HttpErrorCode::Timeout, "timeout"),
            (HttpErrorCode::ResponseTooLarge, "response_too_large"),
            (HttpErrorCode::Denied, "upstream_error"),
            (HttpErrorCode::Tls, "upstream_error"),
        ] {
            let error = transport(&HttpError {
                code,
                message: secret.to_owned(),
            });
            assert_eq!(error.code(), expected);
            assert!(!error.message().contains(secret));
        }
        assert_eq!(status(302).code(), "upstream_error");
        assert_eq!(status(429).code(), "rate_limited");
        assert_eq!(status(503).code(), "maxlag");
        assert_eq!(status(500).code(), "upstream_error");
    }

    #[test]
    fn maps_api_errors_by_operation() {
        assert_eq!(api("maxlag", Operation::Page, false).code(), "maxlag");
        assert_eq!(
            api("badcontinue", Operation::Links, true).code(),
            "invalid_cursor"
        );
        assert_eq!(
            api("missingtitle", Operation::Outline, false).code(),
            "not_found"
        );
        assert_eq!(
            api("missingtitle", Operation::Section, false).code(),
            "no_such_section"
        );
        assert_eq!(
            api("unexpected", Operation::Outline, false).code(),
            "parse_failed"
        );
        assert_eq!(
            api("unexpected", Operation::Search, false).code(),
            "upstream_error"
        );
    }
}
