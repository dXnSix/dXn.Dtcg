//! RFC 6901 JSON Pointers in URI-fragment form, shared by resource fragments,
//! value-level `$ref`s and token-level `$ref`s, so one pointer means the same
//! thing wherever it is interpreted.

use serde_json::Value;

use crate::DtcgError;

/// Splits a local `$ref` (`#/a/b`) into unescaped reference tokens.
pub(crate) fn pointer_parts(pointer: &str) -> Result<Vec<String>, DtcgError> {
    if !pointer.starts_with("#/") {
        return Err(DtcgError::InvalidPointer(pointer.into()));
    }
    fragment_parts(&pointer[1..]).map_err(|_| DtcgError::InvalidPointer(pointer.into()))
}

/// Splits a URI fragment (without `#`): `%XX` sequences are decoded first,
/// then the result is split on `/` and `~1`/`~0` are unescaped (RFC 6901 §6).
/// The empty fragment addresses the whole document.
pub(crate) fn fragment_parts(fragment: &str) -> Result<Vec<String>, DtcgError> {
    let invalid = || DtcgError::InvalidPointer(fragment.into());
    let decoded = percent_decode(fragment).ok_or_else(invalid)?;
    if decoded.is_empty() {
        return Ok(Vec::new());
    }
    let Some(rest) = decoded.strip_prefix('/') else {
        return Err(invalid());
    };
    rest.split('/')
        .map(|part| unescape(part).ok_or_else(invalid))
        .collect()
}

/// Validates a URI fragment without keeping its parts.
pub(crate) fn validate_fragment(fragment: &str) -> Result<(), DtcgError> {
    fragment_parts(fragment).map(|_| ())
}

/// The plain (not percent-encoded) JSON Pointer string of `parts`.
pub(crate) fn plain_pointer(parts: &[String]) -> String {
    parts
        .iter()
        .map(|part| format!("/{}", escape(part)))
        .collect()
}

pub(crate) fn escape(part: &str) -> String {
    part.replace('~', "~0").replace('/', "~1")
}

/// Follows `parts` from `value`. An array index is `0` or a decimal number
/// without a leading zero or sign, and must name an existing element.
pub(crate) fn walk<'a>(mut value: &'a Value, parts: &[String]) -> Option<&'a Value> {
    for part in parts {
        value = match value {
            Value::Object(object) => object.get(part)?,
            Value::Array(array) => array.get(array_index(part)?)?,
            _ => return None,
        };
    }
    Some(value)
}

fn array_index(part: &str) -> Option<usize> {
    let valid = part == "0"
        || (!part.is_empty()
            && !part.starts_with('0')
            && part.bytes().all(|byte| byte.is_ascii_digit()));
    valid.then(|| part.parse().ok()).flatten()
}

fn unescape(value: &str) -> Option<String> {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character == '~' {
            match chars.next() {
                Some('0') => output.push('~'),
                Some('1') => output.push('/'),
                _ => return None,
            }
        } else {
            output.push(character);
        }
    }
    Some(output)
}

fn percent_decode(value: &str) -> Option<String> {
    if !value.contains('%') {
        return Some(value.to_owned());
    }
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes
                .get(index + 1..index + 3)
                .filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))?;
            let hex = std::str::from_utf8(hex).ok()?;
            output.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).ok()
}
