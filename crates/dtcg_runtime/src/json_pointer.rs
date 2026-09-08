use crate::DtcgError;

pub(crate) fn pointer_parts(pointer: &str) -> Result<Vec<String>, DtcgError> {
    if !pointer.starts_with("#/") {
        return Err(DtcgError::InvalidPointer(pointer.into()));
    }
    pointer[2..].split('/').map(unescape).collect()
}

fn unescape(value: &str) -> Result<String, DtcgError> {
    let mut output = String::new();
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character == '~' {
            match chars.next() {
                Some('0') => output.push('~'),
                Some('1') => output.push('/'),
                _ => return Err(DtcgError::InvalidPointer(value.into())),
            }
        } else {
            output.push(character);
        }
    }
    Ok(output)
}
