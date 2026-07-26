use std::{fmt, sync::Arc};

use crate::DtcgError;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct ResourceId(Arc<str>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentReference {
    resource_id: ResourceId,
    fragment: Option<Arc<str>>,
    original: Arc<str>,
}

impl ResourceId {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, DtcgError> {
        let reference = split_reference(value.as_ref())?;
        if reference.path.is_empty() {
            return Err(DtcgError::InvalidResourceId(value.as_ref().into()));
        }
        Ok(Self(normalize(None, reference.path)?.into()))
    }

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
        validate_pointer(fragment)?;
    }
    Ok(ParsedReference {
        path,
        fragment: fragment.filter(|fragment| !fragment.is_empty()),
    })
}

fn normalize(parent: Option<&str>, path: &str) -> Result<String, DtcgError> {
    if path.starts_with('/') || path.starts_with('\\') || path.contains('\\') {
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
            part => parts.push(part.to_owned()),
        }
    }
    if parts.is_empty() {
        return Err(DtcgError::InvalidResourceId(path.into()));
    }
    Ok(parts.join("/"))
}

fn validate_pointer(pointer: &str) -> Result<(), DtcgError> {
    for segment in pointer.trim_start_matches('/').split('/') {
        let mut chars = segment.chars();
        while let Some(character) = chars.next() {
            if character == '~' && !matches!(chars.next(), Some('0' | '1')) {
                return Err(DtcgError::InvalidPointer(pointer.into()));
            }
        }
    }
    Ok(())
}
