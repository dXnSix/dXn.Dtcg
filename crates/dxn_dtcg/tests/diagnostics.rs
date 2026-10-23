#[cfg(feature = "diagnostics")]
use dxn_dtcg::{DtcgDiagnostic, DtcgDiagnosticCode, DtcgError, ResourceId};

#[cfg(not(feature = "diagnostics"))]
#[test]
fn diagnostics_are_opt_in() {
    assert!(!cfg!(feature = "diagnostics"));
}

#[cfg(feature = "diagnostics")]
#[test]
fn diagnostics_expose_logical_context_without_document_bodies() {
    let error = DtcgError::Token {
        resource: ResourceId::parse("tokens/private.json").unwrap(),
        path: "palette.primary".into(),
        message: "expected color, body={secret}".into(),
    };

    let diagnostic = DtcgDiagnostic::from_error(&error);
    assert_eq!(diagnostic.code(), DtcgDiagnosticCode::Token);
    assert_eq!(
        diagnostic.resource().unwrap().as_str(),
        "tokens/private.json"
    );
    assert_eq!(diagnostic.path(), Some("palette.primary"));
    assert_eq!(diagnostic.message(), "expected color, body={secret}");
    assert!(!format!("{diagnostic:?}").contains("private.json\":{\"$value"));
}
