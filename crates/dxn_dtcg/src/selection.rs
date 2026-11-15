//! Resolver inputs (Resolver §5): modifier axes, input matching and
//! normalization.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::{
    DtcgError, ResourceId, SourceOccurrence, SourceOrigin,
    resolver_definition::{declared_modifiers, effective_steps, is_resolver, validate_resolver},
};

/// One resolver input: a modifier with its contexts in authored order and its
/// optional default. Axes are listed in `resolutionOrder` order, followed by
/// the remaining root modifiers in authored order. A `$ref` step's sibling
/// overrides (such as a `default`) are applied.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ModifierAxis {
    pub name: String,
    pub contexts: Vec<String>,
    pub default: Option<String>,
}

/// The modifier axes of the root document. A document without
/// `resolutionOrder` is a token document and has no axes, even when it has a
/// top-level group named `modifiers`.
pub(crate) fn modifier_axes(
    resource: &ResourceId,
    document: &Value,
) -> Result<Vec<ModifierAxis>, DtcgError> {
    if !is_resolver(document) {
        return Ok(Vec::new());
    }
    validate_resolver(resource, document)?;
    let occurrence = SourceOccurrence {
        resource: resource.clone(),
        value: document.clone(),
        origin: SourceOrigin {
            resource: Some(resource.clone()),
            pointer: String::new(),
        },
        member_origins: BTreeMap::new(),
    };
    let steps = effective_steps(&occurrence)?;
    Ok(declared_modifiers(&occurrence, &steps)?
        .into_iter()
        .map(|modifier| ModifierAxis {
            name: modifier.name,
            contexts: modifier.contexts,
            default: modifier.default,
        })
        .collect())
}

/// The outcome of matching an input name against declared names.
pub(crate) enum Match<'a> {
    Found(&'a str),
    Ambiguous(Vec<String>),
    None,
}

/// Matches an input name to a declared name: an exact match wins; otherwise
/// the `str::to_lowercase` forms are compared, and more than one candidate
/// is ambiguous (Resolver §5.1 recommends case-insensitive inputs).
pub(crate) fn match_name<'a>(input: &str, declared: &[&'a str]) -> Match<'a> {
    if let Some(exact) = declared.iter().find(|name| **name == input) {
        return Match::Found(exact);
    }
    let folded = input.to_lowercase();
    let candidates = declared
        .iter()
        .filter(|name| name.to_lowercase() == folded)
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => Match::None,
        [single] => Match::Found(single),
        many => Match::Ambiguous(many.iter().map(|name| (**name).to_owned()).collect()),
    }
}

/// Reads resolver inputs from a JSON object such as `{"theme": "dark"}` into
/// owned `(modifier, context)` pairs. Borrow them for the selection-taking
/// methods with [`selection_refs`].
///
/// ```
/// use dxn_dtcg::{selection_from_json, selection_refs};
///
/// let inputs = selection_from_json(&serde_json::json!({"theme": "dark"}))?;
/// assert_eq!(selection_refs(&inputs), [("theme", "dark")]);
/// # Ok::<(), dxn_dtcg::DtcgError>(())
/// ```
pub fn selection_from_json(inputs: &Value) -> Result<Vec<(String, String)>, DtcgError> {
    let object = inputs.as_object().ok_or_else(|| DtcgError::InvalidInput {
        message: "resolver inputs must be a JSON object".into(),
    })?;
    object
        .iter()
        .map(|(modifier, context)| {
            let context = context.as_str().ok_or_else(|| DtcgError::InvalidInput {
                message: format!("input `{modifier}` must be a string"),
            })?;
            Ok((modifier.clone(), context.to_owned()))
        })
        .collect()
}

/// Borrows owned `(modifier, context)` pairs as the `&[(&str, &str)]`
/// selection every resolving method takes.
pub fn selection_refs(selection: &[(String, String)]) -> Vec<(&str, &str)> {
    selection
        .iter()
        .map(|(modifier, context)| (modifier.as_str(), context.as_str()))
        .collect()
}

/// Every selection of a list of modifier axes: each combination of one
/// context per axis, enumerated lazily, so a large space costs nothing until
/// it is iterated. An empty list of axes has one selection, the empty one.
///
/// ```
/// use dxn_dtcg::{DtcgRuntime, SelectionSpace};
///
/// let runtime = DtcgRuntime::from_json(
///     "r.json",
///     r##"{ "version": "2025.10", "modifiers": {
///       "theme": { "default": "light", "contexts": { "light": [], "dark": [] } },
///       "density": { "default": "comfortable", "contexts": { "comfortable": [], "compact": [] } }
///     }, "resolutionOrder": [{ "$ref": "#/modifiers/theme" }, { "$ref": "#/modifiers/density" }] }"##,
/// )?;
/// let space: SelectionSpace = runtime.selection_space()?.ready().unwrap();
/// assert_eq!(space.combinations(), Some(4));
/// let first = space.iter().next().unwrap();
/// assert_eq!(first, [("theme".into(), "light".into()), ("density".into(), "comfortable".into())]);
/// # Ok::<(), dxn_dtcg::DtcgError>(())
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionSpace {
    axes: Vec<ModifierAxis>,
}

impl SelectionSpace {
    /// The selections of `axes`, in their order.
    pub fn new(axes: Vec<ModifierAxis>) -> Self {
        Self { axes }
    }

    /// The axes, in enumeration order: the first varies slowest.
    pub fn axes(&self) -> &[ModifierAxis] {
        &self.axes
    }

    /// How many selections there are: the product of the axes' context
    /// counts, or `None` when it does not fit a `usize`.
    pub fn combinations(&self) -> Option<usize> {
        self.axes.iter().try_fold(1usize, |product, axis| {
            product.checked_mul(axis.contexts.len())
        })
    }

    /// Every selection, each naming every axis with one of its contexts in
    /// declared spelling: the first axis varies slowest and contexts follow
    /// their authored order.
    pub fn iter(&self) -> Selections<'_> {
        let empty = self.axes.iter().any(|axis| axis.contexts.is_empty());
        Selections {
            axes: &self.axes,
            next: (!empty).then(|| vec![0; self.axes.len()]),
        }
    }
}

impl<'a> IntoIterator for &'a SelectionSpace {
    type Item = Vec<(String, String)>;
    type IntoIter = Selections<'a>;

    fn into_iter(self) -> Selections<'a> {
        self.iter()
    }
}

/// The iterator of [`SelectionSpace::iter`]: owned `(modifier, context)`
/// pairs, which [`selection_refs`] borrows for the resolving methods.
#[derive(Clone, Debug)]
pub struct Selections<'a> {
    axes: &'a [ModifierAxis],
    /// The context index of every axis in the next selection; `None` once
    /// every selection was yielded.
    next: Option<Vec<usize>>,
}

impl Iterator for Selections<'_> {
    type Item = Vec<(String, String)>;

    fn next(&mut self) -> Option<Self::Item> {
        let indexes = self.next.as_mut()?;
        let selection = self
            .axes
            .iter()
            .zip(indexes.iter())
            .map(|(axis, index)| (axis.name.clone(), axis.contexts[*index].clone()))
            .collect();
        // Advance like an odometer: the last axis turns fastest.
        let mut position = indexes.len();
        loop {
            if position == 0 {
                self.next = None;
                break;
            }
            position -= 1;
            indexes[position] += 1;
            if indexes[position] < self.axes[position].contexts.len() {
                break;
            }
            indexes[position] = 0;
        }
        Some(selection)
    }
}
