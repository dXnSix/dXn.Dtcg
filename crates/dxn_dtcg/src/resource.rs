use std::{fmt, sync::Arc};

use crate::{DtcgError, json_pointer::validate_fragment};

/// A fragment-free, portable logical document id such as `themes/base.json`.
/// Callers can join it onto a base directory without re-validating it.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct ResourceId(Arc<str>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentReference {
    resource_id: ResourceId,
    fragment: Option<Arc<str>>,
    original: Arc<str>,
}

impl ResourceId {
    /// Parses and canonicalizes a document id. Ids name whole documents, so a
    /// `#` fragment is rejected rather than dropped; see `normalize` for the
    /// segment rules.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, DtcgError> {
        let value = value.as_ref();
        if value.is_empty() || value.contains('#') {
            return Err(DtcgError::InvalidResourceId(value.into()));
        }
        Ok(Self(normalize(None, value)?.into()))
    }

    /// Resolves a reference (`path#/fragment`, or `#/fragment` for this
    /// document) relative to this id. The fragment is kept separately.
    pub fn resolve(&self, value: impl AsRef<str>) -> Result<DocumentReference, DtcgError> {
        let original = value.as_ref();
        if original.is_empty() {
            return Err(DtcgError::InvalidResourceId(original.into()));
        }
        let reference = split_reference(original)?;
        let resource_id = if reference.path.is_empty() {
            self.clone()
        } else {
            ResourceId(normalize(Some(self.as_str()), reference.path)?.into())
        };
        Ok(DocumentReference {
            resource_id,
            fragment: reference.fragment.map(Arc::from),
            original: original.into(),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ResourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl DocumentReference {
    pub fn resource_id(&self) -> &ResourceId {
        &self.resource_id
    }
    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }
    pub fn original(&self) -> &str {
        &self.original
    }
}

struct ParsedReference<'a> {
    path: &'a str,
    fragment: Option<&'a str>,
}

fn split_reference(value: &str) -> Result<ParsedReference<'_>, DtcgError> {
    let (path, fragment) = value
        .split_once('#')
        .map_or((value, None), |(path, fragment)| (path, Some(fragment)));
    if value.matches('#').count() > 1
        || fragment.is_some_and(|fragment| !fragment.is_empty() && !fragment.starts_with('/'))
    {
        return Err(DtcgError::InvalidPointer(value.into()));
    }
    if let Some(fragment) = fragment {
        validate_fragment(fragment).map_err(|_| DtcgError::InvalidPointer(value.into()))?;
    }
    Ok(ParsedReference {
        path,
        fragment: fragment.filter(|fragment| !fragment.is_empty()),
    })
}

/// Canonicalizes `path` relative to `parent`'s directory. Ids must be portable
/// logical names, so these are rejected:
///
/// - a leading `/`, any `\`, and `:` in any segment: `:` forms Windows drive
///   prefixes (`C:/x`, `c:x`), URI schemes and NTFS alternate data streams, and
///   `..` can move a later segment to the front (`a/../C:/x.json`);
/// - `#`, control characters (NUL included) and the characters Windows
///   reserves: `<`, `>`, `"`, `|`, `?`, `*`;
/// - empty and `.` segments, `..` above the root, and segments ending in `.`
///   or a space;
/// - Windows device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`,
///   `LPT1`–`LPT9`) in any case, with or without an extension.
fn normalize(parent: Option<&str>, path: &str) -> Result<String, DtcgError> {
    if path.starts_with('/')
        || path.contains(|character: char| {
            character.is_control()
                || matches!(
                    character,
                    '\\' | ':' | '#' | '<' | '>' | '"' | '|' | '?' | '*'
                )
        })
    {
        return Err(DtcgError::InvalidResourceId(path.into()));
    }
    let mut parts: Vec<String> = parent
        .and_then(|parent| {
            parent
                .rsplit_once('/')
                .map(|(directory, _)| directory.split('/').map(str::to_owned).collect())
        })
        .unwrap_or_default();
    for part in path.split('/') {
        match part {
            "" | "." => return Err(DtcgError::InvalidResourceId(path.into())),
            ".." => {
                if parts.pop().is_none() {
                    return Err(DtcgError::InvalidResourceId(path.into()));
                }
            }
            part if part.ends_with(['.', ' ']) || is_device_name(part) => {
                return Err(DtcgError::InvalidResourceId(path.into()));
            }
            part => parts.push(part.to_owned()),
        }
    }
    if parts.is_empty() {
        return Err(DtcgError::InvalidResourceId(path.into()));
    }
    Ok(parts.join("/"))
}

/// Whether the segment's stem (the part before the first `.`) is a Windows
/// reserved device name.
fn is_device_name(segment: &str) -> bool {
    let stem = segment
        .split_once('.')
        .map_or(segment, |(stem, _)| stem)
        .to_ascii_uppercase();
    match stem.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" => true,
        _ => {
            let bytes = stem.as_bytes();
            bytes.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && (b'1'..=b'9').contains(&bytes[3])
        }
    }
}
