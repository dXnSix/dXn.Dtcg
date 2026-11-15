//! The structure check across selections: every selection is resolved, and
//! every path that some resolved selection lacks or types differently is
//! reported (Resolver §8 outputs that should share one token structure).

use std::{
    collections::{BTreeMap, btree_map::Entry},
    sync::Arc,
};

use crate::{
    DocumentRequest, DtcgError, DtcgRuntime, Resolution, ResourceId, TokenType, selection_refs,
};

/// The outcome of [`DtcgRuntime::check_structure`]. Fields may be added in
/// minor releases.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct StructureReport {
    /// The selections that resolved, in the order they were given.
    /// [`StructureDifference::types`] is indexed like this list.
    pub selections: Vec<Vec<(String, String)>>,
    /// Every path that is missing from some resolved selection or whose
    /// effective type differs between them, in path order.
    pub differences: Vec<StructureDifference>,
    /// The selections that failed to resolve, with their errors, in the
    /// order they were given. They take no part in the comparison.
    pub failures: Vec<SelectionFailure>,
}

impl StructureReport {
    /// Whether every selection resolved to the same paths with the same
    /// types.
    pub fn is_consistent(&self) -> bool {
        self.differences.is_empty() && self.failures.is_empty()
    }
}

/// A path whose presence or type differs between the resolved selections of
/// a [`StructureReport`]. Fields may be added in minor releases.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct StructureDifference {
    /// The dotted token path.
    pub path: String,
    /// The path's effective type in each of [`StructureReport::selections`],
    /// by index; `None` where the selection has no token at the path.
    pub types: Vec<Option<TokenType>>,
}

impl StructureDifference {
    /// The indexes of the selections that have no token at the path.
    pub fn missing_in(&self) -> Vec<usize> {
        self.types
            .iter()
            .enumerate()
            .filter_map(|(index, token_type)| token_type.is_none().then_some(index))
            .collect()
    }

    /// The types the path has, in order of first appearance; more than one
    /// means the type differs between selections.
    pub fn distinct_types(&self) -> Vec<TokenType> {
        let mut distinct = Vec::new();
        for token_type in self.types.iter().flatten() {
            if !distinct.contains(token_type) {
                distinct.push(*token_type);
            }
        }
        distinct
    }
}

/// A selection that failed to resolve. Fields may be added in minor
/// releases.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SelectionFailure {
    pub selection: Vec<(String, String)>,
    pub error: DtcgError,
}

/// What the resolved selections so far have at one path: the same type in
/// one unbroken run of selections, which a consistent path keeps to the end
/// without storing a type per selection, or else one entry per selection.
enum Seen {
    Run {
        token_type: TokenType,
        first: usize,
        count: usize,
    },
    Each(Vec<Option<TokenType>>),
}

impl Seen {
    /// Records `token_type` for selection `index`, the latest one.
    fn record(&mut self, index: usize, token_type: TokenType) {
        if let Self::Run {
            token_type: run_type,
            first,
            count,
        } = self
            && *run_type == token_type
            && *first + *count == index
        {
            *count += 1;
            return;
        }
        let mut types = std::mem::replace(self, Self::Each(Vec::new())).into_each(index);
        types.push(Some(token_type));
        *self = Self::Each(types);
    }

    /// The entries of the first `len` selections.
    fn into_each(self, len: usize) -> Vec<Option<TokenType>> {
        match self {
            Self::Run {
                token_type,
                first,
                count,
            } => {
                let mut types = vec![None; len];
                for entry in &mut types[first..first + count] {
                    *entry = Some(token_type);
                }
                types
            }
            Self::Each(mut types) => {
                types.resize(len, None);
                types
            }
        }
    }
}

pub(crate) fn check(
    runtime: &DtcgRuntime,
    selections: impl IntoIterator<Item = Vec<(String, String)>>,
) -> Result<Resolution<StructureReport>, DtcgError> {
    let limit = runtime.limits().max_selections;
    let selections = selections
        .into_iter()
        .take(limit.saturating_add(1))
        .collect::<Vec<_>>();
    if selections.len() > limit {
        return Err(DtcgError::LimitExceeded {
            kind: "selections",
            limit,
        });
    }
    let mut report = StructureReport::default();
    let mut missing = BTreeMap::<ResourceId, DocumentRequest>::new();
    let mut seen = BTreeMap::<Arc<str>, Seen>::new();
    for selection in selections {
        let tokens = match runtime.resolve(&selection_refs(&selection)) {
            Ok(Resolution::Ready(tokens)) => tokens,
            Ok(Resolution::Missing(requests)) => {
                for request in requests {
                    missing
                        .entry(request.resource_id.clone())
                        .or_insert(request);
                }
                continue;
            }
            Err(error) => {
                report.failures.push(SelectionFailure { selection, error });
                continue;
            }
        };
        let index = report.selections.len();
        for token in tokens.iter() {
            let token_type = token
                .dtcg_type()
                .expect("resolved tokens have a validated type");
            match seen.entry(token.path.clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(Seen::Run {
                        token_type,
                        first: index,
                        count: 1,
                    });
                }
                Entry::Occupied(mut entry) => entry.get_mut().record(index, token_type),
            }
        }
        report.selections.push(selection);
    }
    if !missing.is_empty() {
        return Ok(Resolution::Missing(missing.into_values().collect()));
    }
    let resolved = report.selections.len();
    report.differences = seen
        .into_iter()
        .filter(
            |(_, seen)| !matches!(seen, Seen::Run { first: 0, count, .. } if *count == resolved),
        )
        .map(|(path, seen)| StructureDifference {
            path: path.to_string(),
            types: seen.into_each(resolved),
        })
        .collect();
    Ok(Resolution::Ready(report))
}
