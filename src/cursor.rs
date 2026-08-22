use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use dekopon_provider_sdk::ProviderError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{error, input::MAX_CURSOR_BYTES};

const CURSOR_VERSION: u8 = 1;
const MAX_CURSOR_DEPTH: u8 = 10;
const MAX_CONTINUATION_BYTES: usize = 1_024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolKind {
    Search,
    Links,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Continuation {
    #[serde(rename = "continue", default, skip_serializing_if = "Option::is_none")]
    pub(crate) generic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) sroffset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) srcontinue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) plcontinue: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    tool: ToolKind,
    fingerprint: String,
    depth: u8,
    continuation: Continuation,
}

#[derive(Debug)]
pub(crate) struct CursorState {
    pub(crate) continuation: Continuation,
    pub(crate) depth: u8,
}

pub(crate) fn decode(
    encoded: &str,
    tool: ToolKind,
    language: &str,
    subject: &str,
    limit: usize,
) -> Result<CursorState, ProviderError> {
    if encoded.is_empty()
        || encoded.len() > MAX_CURSOR_BYTES
        || !encoded
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(error::invalid_cursor());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| error::invalid_cursor())?;
    let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|_| error::invalid_cursor())?;
    if envelope.version != CURSOR_VERSION
        || envelope.tool != tool
        || !(1..=MAX_CURSOR_DEPTH).contains(&envelope.depth)
        || envelope.fingerprint != fingerprint(tool, language, subject, limit)
        || validate_continuation(&envelope.continuation, tool).is_err()
    {
        return Err(error::invalid_cursor());
    }
    Ok(CursorState {
        continuation: envelope.continuation,
        depth: envelope.depth,
    })
}

/// Encodes the continuation for the next API page. At depth ten no further cursor is issued.
pub(crate) fn encode_next(
    continuation: Option<Continuation>,
    tool: ToolKind,
    language: &str,
    subject: &str,
    limit: usize,
    current_depth: u8,
) -> Result<Option<String>, ProviderError> {
    let Some(continuation) = continuation else {
        return Ok(None);
    };
    validate_continuation(&continuation, tool).map_err(|_| error::upstream_error())?;
    if current_depth >= MAX_CURSOR_DEPTH {
        return Ok(None);
    }
    let envelope = Envelope {
        version: CURSOR_VERSION,
        tool,
        fingerprint: fingerprint(tool, language, subject, limit),
        depth: current_depth + 1,
        continuation,
    };
    let json = serde_json::to_vec(&envelope).map_err(|_| error::upstream_error())?;
    let encoded = URL_SAFE_NO_PAD.encode(json);
    if encoded.len() > MAX_CURSOR_BYTES {
        return Err(error::response_too_large());
    }
    Ok(Some(encoded))
}

fn validate_continuation(continuation: &Continuation, tool: ToolKind) -> Result<(), ProviderError> {
    for value in [
        continuation.generic.as_deref(),
        continuation.srcontinue.as_deref(),
        continuation.plcontinue.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if value.is_empty()
            || value.len() > MAX_CONTINUATION_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(error::invalid_cursor());
        }
    }
    if continuation
        .sroffset
        .is_some_and(|offset| offset > 10_000_000)
    {
        return Err(error::invalid_cursor());
    }
    let valid = match tool {
        ToolKind::Search => {
            continuation.plcontinue.is_none()
                && (continuation.sroffset.is_some() || continuation.srcontinue.is_some())
        }
        ToolKind::Links => {
            continuation.sroffset.is_none()
                && continuation.srcontinue.is_none()
                && continuation.plcontinue.is_some()
        }
    };
    if !valid {
        return Err(error::invalid_cursor());
    }
    Ok(())
}

fn fingerprint(tool: ToolKind, language: &str, subject: &str, limit: usize) -> String {
    let tool = match tool {
        ToolKind::Search => "search",
        ToolKind::Links => "links",
    };
    let mut digest = Sha256::new();
    for value in [
        "dekopon-mediawiki-cursor-v1",
        tool,
        language,
        subject,
        &limit.to_string(),
    ] {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    URL_SAFE_NO_PAD.encode(&digest.finalize()[..18])
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use serde_json::Value;

    use super::{Continuation, MAX_CURSOR_DEPTH, ToolKind, decode, encode_next};

    fn search_continuation() -> Continuation {
        Continuation {
            generic: Some("-||".to_owned()),
            sroffset: Some(5),
            ..Continuation::default()
        }
    }

    #[test]
    fn round_trip_binds_every_request_dimension() {
        let encoded = encode_next(
            Some(search_continuation()),
            ToolKind::Search,
            "en",
            "Ada Lovelace",
            5,
            0,
        )
        .expect("encodes")
        .expect("cursor exists");
        let decoded = decode(&encoded, ToolKind::Search, "en", "Ada Lovelace", 5)
            .expect("matching request decodes");
        assert_eq!(decoded.depth, 1);
        assert_eq!(decoded.continuation.sroffset, Some(5));

        for (tool, language, subject, limit) in [
            (ToolKind::Links, "en", "Ada Lovelace", 5),
            (ToolKind::Search, "de", "Ada Lovelace", 5),
            (ToolKind::Search, "en", "Ada Byron", 5),
            (ToolKind::Search, "en", "Ada Lovelace", 6),
        ] {
            assert!(decode(&encoded, tool, language, subject, limit).is_err());
        }
    }

    #[test]
    fn unknown_or_cross_tool_continuation_fields_fail_closed() {
        let encoded = encode_next(
            Some(search_continuation()),
            ToolKind::Search,
            "en",
            "Ada",
            5,
            0,
        )
        .expect("encodes")
        .expect("cursor exists");
        let bytes = URL_SAFE_NO_PAD.decode(&encoded).expect("base64");
        let mut value: Value = serde_json::from_slice(&bytes).expect("JSON");
        value["continuation"]["unexpected"] = Value::String("x".to_owned());
        let tampered = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).expect("JSON"));
        assert!(decode(&tampered, ToolKind::Search, "en", "Ada", 5).is_err());

        let invalid_links = Continuation {
            plcontinue: Some("974|0|Next".to_owned()),
            sroffset: Some(5),
            ..Continuation::default()
        };
        assert!(encode_next(Some(invalid_links), ToolKind::Links, "en", "Ada", 5, 0).is_err());
    }

    #[test]
    fn pagination_stops_before_issuing_depth_eleven() {
        let next = encode_next(
            Some(search_continuation()),
            ToolKind::Search,
            "en",
            "Ada",
            5,
            MAX_CURSOR_DEPTH,
        )
        .expect("depth cap is a successful terminal page");
        assert!(next.is_none());
    }
}
