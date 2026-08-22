use dekopon_provider_sdk::{ComponentResponse, ProviderError};
use serde::Serialize;
use serde_json::Value;

use crate::error;

pub(crate) const MAX_PROJECTED_OUTPUT_BYTES: usize = 14_000;
pub(crate) const MAX_SDK_ENVELOPE_BYTES: usize = 16_384;

/// Collapses all whitespace into single ASCII spaces and removes leading/trailing space.
pub(crate) fn compact_plain_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut pending_space = false;
    for character in value.chars() {
        if character.is_whitespace() || character.is_control() {
            pending_space = !output.is_empty();
        } else {
            if pending_space {
                output.push(' ');
            }
            output.push(character);
            pending_space = false;
        }
    }
    output
}

/// Truncates to both a Unicode-scalar and UTF-8-byte ceiling without splitting a scalar.
pub(crate) fn truncate_text(
    value: &str,
    max_characters: usize,
    max_bytes: usize,
) -> (String, bool) {
    let mut end = 0;
    for (count, (index, character)) in value.char_indices().enumerate() {
        if count == max_characters || index + character.len_utf8() > max_bytes {
            break;
        }
        end = index + character.len_utf8();
    }
    let truncated = end < value.len();
    (value[..end].to_owned(), truncated)
}

pub(crate) fn truncate_bytes_in_place(value: &mut String, max_bytes: usize) -> bool {
    if value.len() <= max_bytes {
        return false;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    true
}

pub(crate) fn serialized_len<T: Serialize>(value: &T) -> Result<usize, ProviderError> {
    serde_json::to_vec(value)
        .map(|encoded| encoded.len())
        .map_err(|_| error::upstream_error())
}

pub(crate) fn projected_fits<T: Serialize>(value: &T) -> bool {
    serialized_len(value).is_ok_and(|length| length <= MAX_PROJECTED_OUTPUT_BYTES)
}

/// Converts a typed projection to JSON and checks the exact SDK success envelope size.
pub(crate) fn finish<T: Serialize>(output: &T) -> Result<Value, ProviderError> {
    let output = serde_json::to_value(output).map_err(|_| error::upstream_error())?;
    if serialized_len(&output)? > MAX_PROJECTED_OUTPUT_BYTES {
        return Err(error::response_too_large());
    }
    let envelope = ComponentResponse::Succeeded {
        output: output.clone(),
    };
    if serialized_len(&envelope)? > MAX_SDK_ENVELOPE_BYTES {
        return Err(error::response_too_large());
    }
    Ok(output)
}

/// Chooses a materially smaller byte target after an oversized serialization.
pub(crate) fn next_text_target(current_bytes: usize, serialized_bytes: usize) -> usize {
    let excess = serialized_bytes.saturating_sub(MAX_PROJECTED_OUTPUT_BYTES);
    current_bytes.saturating_sub(excess.max(64))
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use super::{
        MAX_PROJECTED_OUTPUT_BYTES, MAX_SDK_ENVELOPE_BYTES, compact_plain_text, finish,
        serialized_len, truncate_text,
    };

    #[derive(Serialize)]
    struct Text<'a> {
        text: &'a str,
    }

    #[test]
    fn truncation_is_scalar_and_byte_safe() {
        assert_eq!(truncate_text("abcdef", 6, 6), ("abcdef".to_owned(), false));
        assert_eq!(truncate_text("abcdefg", 6, 99), ("abcdef".to_owned(), true));
        assert_eq!(truncate_text("ab😀cd", 99, 5), ("ab".to_owned(), true));
        assert_eq!(truncate_text("😀😀", 1, 99), ("😀".to_owned(), true));
    }

    #[test]
    fn plain_text_collapses_controls_and_unicode_whitespace() {
        assert_eq!(
            compact_plain_text("  Ada\n\tLovelace\u{a0}  wrote  "),
            "Ada Lovelace wrote"
        );
    }

    #[test]
    fn exact_json_and_sdk_envelope_limits_include_escaping() {
        let escaping = "\\\"".repeat(3_400);
        let projected = Text { text: &escaping };
        assert!(serialized_len(&projected).expect("serializes") < MAX_PROJECTED_OUTPUT_BYTES);
        let value = finish(&projected).expect("bounded output succeeds");
        let envelope = dekopon_provider_sdk::ComponentResponse::Succeeded { output: value };
        assert!(serialized_len(&envelope).expect("serializes") <= MAX_SDK_ENVELOPE_BYTES);

        let oversized = Text {
            text: &"😀".repeat(4_000),
        };
        assert_eq!(
            finish(&oversized)
                .expect_err("four-byte output is rejected")
                .code(),
            "response_too_large"
        );
    }
}
