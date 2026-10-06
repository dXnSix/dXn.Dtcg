//! The official DTCG 2025.10 conformance corpus, vendored under
//! `tests/fixtures/dtcg-test-suite/2025.10/` (see its `README.md`).
//!
//! Every manifest entry is evaluated. The set of fixtures whose expectation the
//! runtime does not meet must equal `KNOWN_GAPS` exactly: fixing a gap without
//! removing its id fails the test, and so does any new failure.

use std::{
    collections::BTreeSet,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
};

use dtcg_runtime::{DtcgError, DtcgRuntime, Resolution};
use serde_json::{Value, json};

/// Manifest ids of fixtures whose expectation is not met yet, sorted.
const KNOWN_GAPS: &[&str] = &[
    "empty-resolution-order",
    "modifier-single-context",
    "modifier-with-additional-properties",
    "root-with-additional-properties",
    "set-with-additional-properties",
    "typography-lineHeight-wrong-type",
];

const FORMAT_FIXTURES: usize = 220;
const RESOLVER_FIXTURES: usize = 30;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Suite {
    Format,
    Resolver,
}

struct Fixture {
    suite: Suite,
    id: String,
    purpose: String,
    input: PathBuf,
    positive: bool,
}

enum Outcome {
    Accepted,
    Rejected(String),
    Missing(String),
    Panicked(String),
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accepted => formatter.write_str("accepted"),
            Self::Rejected(error) => write!(formatter, "rejected: {error}"),
            Self::Missing(documents) => write!(formatter, "missing documents: {documents}"),
            Self::Panicked(message) => write!(formatter, "panicked: {message}"),
        }
    }
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dtcg-test-suite/2025.10")
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn fixtures() -> Vec<Fixture> {
    let root = corpus_root();
    let index = read_json(&root.join("manifest.json"));
    let mut fixtures = Vec::new();
    for manifest in index["manifests"].as_array().expect("manifest list") {
        let suite = match manifest["id"].as_str() {
            Some("format") => Suite::Format,
            Some("resolver") => Suite::Resolver,
            other => panic!("unknown manifest {other:?}"),
        };
        let file = root.join(manifest["file"].as_str().expect("manifest file"));
        let directory = file.parent().expect("manifest directory").to_owned();
        for test in read_json(&file)["tests"]
            .as_array()
            .expect("manifest tests")
        {
            let text = |key: &str| {
                test[key]
                    .as_str()
                    .unwrap_or_else(|| panic!("manifest entry without `{key}`: {test}"))
                    .to_owned()
            };
            let positive = match text("type").as_str() {
                "PositiveEvaluationTest" => true,
                "NegativeEvaluationTest" => false,
                other => panic!("unknown test type `{other}`"),
            };
            fixtures.push(Fixture {
                suite,
                id: text("id"),
                purpose: text("purpose"),
                input: directory.join(text("input")),
                positive,
            });
        }
    }
    fixtures
}

fn describe(result: Result<Resolution<impl Sized>, DtcgError>) -> Result<(), Outcome> {
    match result {
        Ok(Resolution::Ready(_)) => Ok(()),
        Ok(Resolution::Missing(requests)) => Err(Outcome::Missing(
            requests
                .iter()
                .map(|request| request.resource_id.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        )),
        Err(error) => Err(Outcome::Rejected(error.to_string())),
    }
}

fn evaluate_format(runtime: DtcgRuntime) -> Result<(), Outcome> {
    describe(runtime.resolve(&[]))
}

/// Accepted when the resolver validates completely and then resolves with each
/// modifier's default context, or its first context when it has no default.
fn evaluate_resolver(runtime: DtcgRuntime) -> Result<(), Outcome> {
    describe(runtime.validate_complete())?;
    let axes = match runtime.modifiers() {
        Ok(Resolution::Ready(axes)) => axes,
        other => return describe(other),
    };
    let selection = axes
        .iter()
        .filter_map(|axis| {
            axis.default
                .clone()
                .or_else(|| axis.contexts.first().cloned())
                .map(|context| (axis.name.clone(), context))
        })
        .collect::<Vec<_>>();
    let selection = selection
        .iter()
        .map(|(name, context)| (name.as_str(), context.as_str()))
        .collect::<Vec<_>>();
    describe(runtime.resolve(&selection))
}

fn evaluate(fixture: &Fixture) -> Outcome {
    let json = std::fs::read_to_string(&fixture.input)
        .unwrap_or_else(|error| panic!("read {}: {error}", fixture.input.display()));
    let suite = fixture.suite;
    let result = catch_unwind(AssertUnwindSafe(move || {
        let runtime = DtcgRuntime::from_json("fixture.json", json)
            .map_err(|error| Outcome::Rejected(error.to_string()))?;
        match suite {
            Suite::Format => evaluate_format(runtime),
            Suite::Resolver => evaluate_resolver(runtime),
        }
    }));
    match result {
        Ok(Ok(())) => Outcome::Accepted,
        Ok(Err(outcome)) => outcome,
        Err(payload) => Outcome::Panicked(
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|text| (*text).into()))
                .unwrap_or_default(),
        ),
    }
}

/// A positive fixture must be accepted; a negative fixture must fail with
/// `Err`, never with `Missing` and never with a panic.
fn meets_expectation(fixture: &Fixture, outcome: &Outcome) -> bool {
    match outcome {
        Outcome::Accepted => fixture.positive,
        Outcome::Rejected(_) => !fixture.positive,
        Outcome::Missing(_) | Outcome::Panicked(_) => false,
    }
}

#[test]
fn official_corpus_matches_the_known_gaps_exactly() {
    assert!(
        KNOWN_GAPS.windows(2).all(|pair| pair[0] < pair[1]),
        "KNOWN_GAPS must be sorted and unique"
    );
    let fixtures = fixtures();
    assert_eq!(
        fixtures
            .iter()
            .filter(|fixture| fixture.suite == Suite::Format)
            .count(),
        FORMAT_FIXTURES
    );
    assert_eq!(
        fixtures
            .iter()
            .filter(|fixture| fixture.suite == Suite::Resolver)
            .count(),
        RESOLVER_FIXTURES
    );

    let known = KNOWN_GAPS.iter().copied().collect::<BTreeSet<_>>();
    let mut failing = BTreeSet::new();
    let mut report = Vec::new();
    for fixture in &fixtures {
        let outcome = evaluate(fixture);
        let failed = !meets_expectation(fixture, &outcome);
        if failed {
            failing.insert(fixture.id.as_str());
        }
        if failed != known.contains(fixture.id.as_str()) {
            let expectation = if fixture.positive { "accept" } else { "reject" };
            let status = if failed {
                "new failure"
            } else {
                "fixed gap still listed in KNOWN_GAPS"
            };
            report.push(format!(
                "- {} ({status}; expected to {expectation}): {}\n  observed: {outcome}",
                fixture.id, fixture.purpose
            ));
        }
    }
    let unknown = known
        .iter()
        .filter(|id| !fixtures.iter().any(|fixture| fixture.id == **id))
        .collect::<Vec<_>>();
    assert!(
        unknown.is_empty(),
        "KNOWN_GAPS names fixtures the manifests do not list: {unknown:?}"
    );

    println!(
        "official corpus: {} passed, {} known gaps",
        fixtures.len() - failing.len(),
        known.len()
    );
    assert!(
        report.is_empty(),
        "official corpus mismatches against KNOWN_GAPS:\n{}",
        report.join("\n")
    );
}

#[test]
fn vendored_fixtures_keep_the_upstream_line_endings() {
    fn visit(directory: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, files);
            } else {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    visit(&corpus_root(), &mut files);
    assert_eq!(files.len(), 3 + FORMAT_FIXTURES + RESOLVER_FIXTURES);
    let converted = files
        .iter()
        .filter(|path| std::fs::read(path).unwrap().contains(&b'\r'))
        .collect::<Vec<_>>();
    assert!(
        converted.is_empty(),
        "vendored files must stay byte-identical to upstream (LF): {converted:?}"
    );
}

#[test]
fn a_string_schema_is_accepted_and_ignored_at_a_document_root() {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        json!({
            "$schema": "https://www.designtokens.org/schemas/2025.10/format.json",
            "size": {"$type": "number", "$value": 4}
        })
        .to_string(),
    )
    .unwrap();
    let tokens = runtime.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens.get("size").unwrap().value, json!(4));

    let mut layered = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "resolutionOrder": [{"type": "set", "name": "base", "sources": [{"$ref": "base.json"}]}]
        })
        .to_string(),
    )
    .unwrap();
    layered
        .provide_json(
            "base.json",
            json!({"$schema": "format.json", "size": {"$type": "number", "$value": 8}}).to_string(),
        )
        .unwrap();
    let tokens = layered.resolve(&[]).unwrap().ready().unwrap();
    assert_eq!(tokens.get("size").unwrap().value, json!(8));
}

#[test]
fn schema_is_rejected_off_the_document_root_or_when_not_a_string() {
    let error = |document: Value| {
        DtcgRuntime::from_json("tokens.json", document.to_string())
            .unwrap()
            .resolve(&[])
            .unwrap_err()
    };
    let non_string = error(json!({"$schema": 1, "size": {"$type": "number", "$value": 4}}));
    assert!(
        matches!(&non_string, DtcgError::Token { path, message, .. }
            if path.is_empty() && message == "`$schema` must be a string"),
        "{non_string:?}"
    );
    let on_group =
        error(json!({"size": {"$schema": "x", "small": {"$type": "number", "$value": 4}}}));
    assert!(
        matches!(&on_group, DtcgError::Token { path, message, .. }
            if path == "size" && message == "unknown reserved property `$schema`"),
        "{on_group:?}"
    );
    let on_token = error(json!({"size": {"$schema": "x", "$type": "number", "$value": 4}}));
    assert!(
        matches!(&on_token, DtcgError::Token { path, message, .. }
            if path == "size" && message == "unknown reserved property `$schema`"),
        "{on_token:?}"
    );
}

#[test]
fn schema_in_fragment_and_inline_sources_is_not_a_document_root() {
    let mut fragment = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "resolutionOrder": [{"type": "set", "name": "base", "sources": [{"$ref": "library.json#/palette"}]}]
        })
        .to_string(),
    )
    .unwrap();
    fragment
        .provide_json(
            "library.json",
            json!({"palette": {"$schema": "x", "red": {"$type": "number", "$value": 1}}})
                .to_string(),
        )
        .unwrap();
    assert!(matches!(
        fragment.resolve(&[]),
        Err(DtcgError::Token { message, .. }) if message == "unknown reserved property `$schema`"
    ));

    let inline = DtcgRuntime::from_json(
        "root.json",
        json!({
            "version": "2025.10",
            "resolutionOrder": [{"type": "set", "name": "base", "sources": [
                {"$schema": "x", "red": {"$type": "number", "$value": 1}}
            ]}]
        })
        .to_string(),
    )
    .unwrap();
    assert!(matches!(
        inline.resolve(&[]),
        Err(DtcgError::Token { message, .. }) if message == "unknown reserved property `$schema`"
    ));

    let tokens = DtcgRuntime::from_json(
        "tokens.json",
        json!({"a": {"$type": "number", "$value": 1}}).to_string(),
    )
    .unwrap();
    assert!(matches!(
        tokens.preview_source(&json!({"$schema": "x", "b": {"$type": "number", "$value": 2}}), &[]),
        Err(DtcgError::Token { message, .. }) if message == "unknown reserved property `$schema`"
    ));
}
