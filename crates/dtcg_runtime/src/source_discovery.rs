use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::{
    DocumentCatalog, DocumentRequest, DtcgError, Resolution, ResourceId, RuntimeLimits,
    SourceOccurrence, SourceOrigin,
    catalog::parse_document,
    error::invalid,
    json_pointer::{fragment_parts, plain_pointer, pointer_parts, walk},
    resolver_definition::{
        Step, StepKind, all_context_documents, context_documents, declared_modifiers,
        effective_steps, is_resolver, resolve_local_reference, set_documents, validate_resolver,
    },
    selection::{Match, match_name},
};

pub(crate) fn required_root(root: Option<&ResourceId>) -> Result<&ResourceId, DtcgError> {
    root.ok_or_else(|| {
        invalid(
            &ResourceId::parse("catalog").expect("static logical id"),
            "root document is not set",
        )
    })
}

/// The terminal sources of one discovery, how it used the inputs, and the
/// context it chose for every modifier it reached, in declaration order
/// (root first) and with declared spellings.
pub(crate) struct Discovered {
    pub(crate) sources: Vec<SourceOccurrence>,
    pub(crate) inputs: InputUse,
    pub(crate) choices: Vec<(String, String)>,
}

/// Which inputs a discovery matched to a declared modifier, and every
/// modifier name the resolvers it reached declare.
#[derive(Default)]
pub(crate) struct InputUse {
    claimed: BTreeSet<usize>,
    /// Declared modifier names in first-declared order, root first.
    declared: Vec<String>,
}

impl InputUse {
    /// Whether every one of `count` inputs matched a declared modifier.
    pub(crate) fn claims_all(&self, count: usize) -> bool {
        self.claimed.len() == count
    }
}

/// Fails for an input that no reached resolver declares. A graph that
/// declares no modifiers ignores inputs (Resolver §6.1).
pub(crate) fn check_inputs(
    selection: &[(&str, &str)],
    uses: &[&InputUse],
) -> Result<(), DtcgError> {
    let mut declared = Vec::<String>::new();
    for name in uses.iter().flat_map(|used| &used.declared) {
        if !declared.contains(name) {
            declared.push(name.clone());
        }
    }
    if declared.is_empty() {
        return Ok(());
    }
    for (index, (modifier, _)) in selection.iter().enumerate() {
        if !uses.iter().any(|used| used.claimed.contains(&index)) {
            return Err(DtcgError::UnknownModifier {
                modifier: (*modifier).into(),
                known: declared,
            });
        }
    }
    Ok(())
}

pub(crate) fn discover(
    catalog: &DocumentCatalog,
    root: &ResourceId,
    limits: &RuntimeLimits,
    selection: &[(&str, &str)],
    include_inactive: bool,
) -> Result<Resolution<Vec<SourceOccurrence>>, DtcgError> {
    checked(
        discover_with_inputs(catalog, root, limits, selection, include_inactive)?,
        selection,
        include_inactive,
    )
}

/// Like [`discover`], leaving the check for undeclared inputs to the caller.
pub(crate) fn discover_with_inputs(
    catalog: &DocumentCatalog,
    root: &ResourceId,
    limits: &RuntimeLimits,
    selection: &[(&str, &str)],
    include_inactive: bool,
) -> Result<Resolution<Discovered>, DtcgError> {
    let mut discovery = Discovery::new(catalog, limits, selection, include_inactive)?;
    let root = discovery.load(root, None, root, root.as_str())?;
    discovery.expand(root.into_iter().map(|source| (source, 0)).collect())
}

fn checked(
    discovered: Resolution<Discovered>,
    selection: &[(&str, &str)],
    include_inactive: bool,
) -> Result<Resolution<Vec<SourceOccurrence>>, DtcgError> {
    match discovered {
        Resolution::Ready(discovered) => {
            if !include_inactive {
                check_inputs(selection, &[&discovered.inputs])?;
            }
            Ok(Resolution::Ready(discovered.sources))
        }
        Resolution::Missing(requests) => Ok(Resolution::Missing(requests)),
    }
}

/// Discover a supplied source in the root resolver's logical reference scope.
pub(crate) fn discover_source(
    catalog: &DocumentCatalog,
    root: &ResourceId,
    limits: &RuntimeLimits,
    source: &Value,
    selection: &[(&str, &str)],
    include_inactive: bool,
) -> Result<Resolution<Vec<SourceOccurrence>>, DtcgError> {
    checked(
        discover_source_with_inputs(catalog, root, limits, source, selection, include_inactive)?,
        selection,
        include_inactive,
    )
}

/// Like [`discover_source`], leaving the check for undeclared inputs to the
/// caller.
pub(crate) fn discover_source_with_inputs(
    catalog: &DocumentCatalog,
    root: &ResourceId,
    limits: &RuntimeLimits,
    source: &Value,
    selection: &[(&str, &str)],
    include_inactive: bool,
) -> Result<Resolution<Discovered>, DtcgError> {
    let mut discovery = Discovery::new(catalog, limits, selection, include_inactive)?;
    let Some(scope) = discovery.load(root, None, root, root.as_str())? else {
        return discovery.expand(Vec::new());
    };
    if is_resolver(&scope.value) {
        validate_resolver(&scope.resource, &scope.value)?;
        let steps = effective_steps(&scope)?;
        if !include_inactive {
            discovery.choose_contexts(&scope, &steps)?;
        }
    }
    let supplied = SourceOccurrence {
        resource: root.clone(),
        value: source.clone(),
        origin: SourceOrigin {
            resource: None,
            pointer: String::new(),
        },
        member_origins: BTreeMap::new(),
    };
    let mut sources = Vec::new();
    discovery.append_source(&scope, supplied, &mut sources, &mut Vec::new(), 0)?;
    discovery.expand(sources)
}

struct Discovery<'a> {
    catalog: &'a DocumentCatalog,
    limits: &'a RuntimeLimits,
    selection: &'a [(&'a str, &'a str)],
    inputs: InputUse,
    choices: Vec<(String, String)>,
    include_inactive: bool,
    missing: BTreeMap<ResourceId, DocumentRequest>,
    /// Expansion steps so far: set references, local references, document
    /// loads and nested resolvers, including those that contribute no source.
    expansions: usize,
}

impl<'a> Discovery<'a> {
    fn new(
        catalog: &'a DocumentCatalog,
        limits: &'a RuntimeLimits,
        selection: &'a [(&'a str, &'a str)],
        include_inactive: bool,
    ) -> Result<Self, DtcgError> {
        // A modifier selected twice, exactly or case-insensitively, is
        // reported by the resolver that declares it.
        Ok(Self {
            catalog,
            limits,
            selection,
            inputs: InputUse::default(),
            choices: Vec::new(),
            include_inactive,
            missing: BTreeMap::new(),
            expansions: 0,
        })
    }

    fn load(
        &mut self,
        resource: &ResourceId,
        fragment: Option<&str>,
        from: &ResourceId,
        reference: &str,
    ) -> Result<Option<SourceOccurrence>, DtcgError> {
        let Some(json) = self.catalog.get(resource) else {
            self.missing
                .entry(resource.clone())
                .or_insert_with(|| DocumentRequest {
                    resource_id: resource.clone(),
                    referenced_from: from.clone(),
                    original_reference: reference.into(),
                });
            self.check_count(self.missing.len())?;
            return Ok(None);
        };
        let mut value = parse_document(resource, json)?;
        let mut pointer = String::new();
        if let Some(fragment) = fragment {
            let parts = fragment_parts(fragment)?;
            value = walk(&value, &parts)
                .cloned()
                .ok_or_else(|| DtcgError::InvalidPointer(fragment.into()))?;
            pointer = plain_pointer(&parts);
        }
        Ok(Some(SourceOccurrence {
            resource: resource.clone(),
            value,
            origin: SourceOrigin {
                resource: Some(resource.clone()),
                pointer,
            },
            member_origins: BTreeMap::new(),
        }))
    }

    fn expand(
        mut self,
        sources: Vec<(SourceOccurrence, usize)>,
    ) -> Result<Resolution<Discovered>, DtcgError> {
        self.check_count(sources.len())?;
        let mut queue = sources
            .into_iter()
            .rev()
            .map(|(source, depth)| (source, depth, Vec::new()))
            .collect::<Vec<_>>();
        let mut terminal = Vec::new();
        while let Some((source, depth, mut ancestors)) = queue.pop() {
            self.check_depth(depth)?;
            if !is_resolver(&source.value) {
                terminal.push(source);
                continue;
            }
            if ancestors.contains(&source.origin) {
                return Err(DtcgError::DocumentCycle(source.resource.to_string()));
            }
            self.count_expansion()?;
            let children = self.resolver_sources(&source, depth + 1)?;
            // Identity is tracked along this occurrence's ancestry only. Repeated
            // resolver sources must still participate at their authored position.
            ancestors.push(source.origin);
            for (child, child_depth) in children.into_iter().rev() {
                queue.push((child, child_depth, ancestors.clone()));
            }
            self.check_count(queue.len().saturating_add(terminal.len()))?;
        }
        if self.missing.is_empty() {
            Ok(Resolution::Ready(Discovered {
                sources: terminal,
                inputs: self.inputs,
                choices: self.choices,
            }))
        } else {
            Ok(Resolution::Missing(self.missing.into_values().collect()))
        }
    }

    fn resolver_sources(
        &mut self,
        source: &SourceOccurrence,
        depth: usize,
    ) -> Result<Vec<(SourceOccurrence, usize)>, DtcgError> {
        validate_resolver(&source.resource, &source.value)?;
        let steps = effective_steps(source)?;
        let contexts = if self.include_inactive {
            BTreeMap::new()
        } else {
            self.choose_contexts(source, &steps)?
        };
        let mut result = Vec::new();
        for step in &steps {
            let documents = match step.kind {
                StepKind::Set => set_documents(&step.definition)?,
                StepKind::Modifier if self.include_inactive => {
                    all_context_documents(&step.definition)?
                }
                StepKind::Modifier => context_documents(&step.definition, &contexts[&step.name])?,
            };
            for document in documents {
                self.append_source(source, document, &mut result, &mut Vec::new(), depth)?;
            }
        }
        Ok(result)
    }

    /// Chooses the context of every modifier `resolver` declares: the input
    /// that names it (matched case-insensitively among this resolver's
    /// modifiers), or else its default. Records the matched inputs and the
    /// declared names; an input this resolver does not declare is left for
    /// the other reached resolvers.
    fn choose_contexts(
        &mut self,
        resolver: &SourceOccurrence,
        steps: &[Step],
    ) -> Result<BTreeMap<String, String>, DtcgError> {
        let modifiers = declared_modifiers(resolver, steps)?;
        let names = modifiers
            .iter()
            .map(|modifier| modifier.name.as_str())
            .collect::<Vec<_>>();
        let mut selected = BTreeMap::<&str, &str>::new();
        for (index, (input, context)) in self.selection.iter().enumerate() {
            match match_name(input, &names) {
                Match::Found(name) => {
                    if selected.insert(name, context).is_some() {
                        let modifier =
                            &modifiers[names.iter().position(|n| *n == name).expect("declared")];
                        return Err(DtcgError::UnknownContext {
                            modifier: name.into(),
                            context: (*context).into(),
                            allowed: modifier.contexts.clone(),
                            message: "modifier is selected twice".into(),
                        });
                    }
                    self.inputs.claimed.insert(index);
                }
                Match::Ambiguous(candidates) => {
                    return Err(DtcgError::UnknownModifier {
                        modifier: (*input).into(),
                        known: candidates,
                    });
                }
                Match::None => {}
            }
        }
        for name in &names {
            if !self.inputs.declared.iter().any(|declared| declared == name) {
                self.inputs.declared.push((*name).to_owned());
            }
        }
        let mut contexts = BTreeMap::new();
        for modifier in &modifiers {
            let context = match selected.get(modifier.name.as_str()) {
                Some(input) => {
                    let declared = modifier
                        .contexts
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>();
                    match match_name(input, &declared) {
                        Match::Found(context) => context.to_owned(),
                        Match::Ambiguous(candidates) => {
                            return Err(DtcgError::UnknownContext {
                                modifier: modifier.name.clone(),
                                context: (*input).into(),
                                allowed: candidates,
                                message: "context matches more than one declared context".into(),
                            });
                        }
                        Match::None => {
                            return Err(DtcgError::UnknownContext {
                                modifier: modifier.name.clone(),
                                context: (*input).into(),
                                allowed: modifier.contexts.clone(),
                                message: "selected modifier context is unknown".into(),
                            });
                        }
                    }
                }
                None => modifier
                    .default
                    .clone()
                    .ok_or_else(|| DtcgError::UnknownContext {
                        modifier: modifier.name.clone(),
                        context: String::new(),
                        allowed: modifier.contexts.clone(),
                        message: "modifier has no default and was not selected".into(),
                    })?,
            };
            if !self.choices.iter().any(|(name, _)| *name == modifier.name) {
                self.choices.push((modifier.name.clone(), context.clone()));
            }
            contexts.insert(modifier.name.clone(), context);
        }
        Ok(contexts)
    }

    fn append_source(
        &mut self,
        scope: &SourceOccurrence,
        source: SourceOccurrence,
        result: &mut Vec<(SourceOccurrence, usize)>,
        references: &mut Vec<String>,
        depth: usize,
    ) -> Result<(), DtcgError> {
        self.check_depth(depth)?;
        let object = source
            .value
            .as_object()
            .ok_or_else(|| invalid(&scope.resource, "document source must be an object"))?;
        let Some(reference) = object.get("$ref") else {
            result.push((source, depth));
            return self.check_count(result.len());
        };
        let reference = reference
            .as_str()
            .ok_or_else(|| invalid(&scope.resource, "reference object `$ref` must be a string"))?;
        if reference.starts_with('#') && targets_forbidden_member(reference)? {
            return Err(invalid(
                &scope.resource,
                &format!("forbidden source reference `{reference}`"),
            ));
        }
        self.count_expansion()?;
        if reference.starts_with('#') {
            if references.iter().any(|active| active == reference) {
                return Err(invalid(
                    &scope.resource,
                    &format!("reference cycle at `{reference}`"),
                ));
            }
            references.push(reference.into());
            let parts = pointer_parts(reference)?;
            if let [kind, name] = parts.as_slice()
                && kind == "sets"
            {
                if scope
                    .value
                    .get("sets")
                    .and_then(|sets| sets.get(name))
                    .is_none()
                {
                    return Err(invalid(&scope.resource, &format!("unknown set `{name}`")));
                }
                let set = resolve_local_reference(scope, &source)?;
                for source in set_documents(&set)? {
                    self.append_source(scope, source, result, references, depth + 1)?;
                }
            } else {
                let resolved = resolve_local_reference(scope, &source)?;
                self.append_source(scope, resolved, result, references, depth + 1)?;
            }
            references.pop();
        } else {
            let from = source
                .member_origin("$ref")
                .resource
                .as_ref()
                .unwrap_or(&scope.resource);
            let target = from.resolve(reference)?;
            if let Some(loaded) =
                self.load(target.resource_id(), target.fragment(), from, reference)?
            {
                result.push((loaded.overlay(&source)?, depth));
                self.check_count(result.len())?;
            }
        }
        Ok(())
    }

    fn check_depth(&self, depth: usize) -> Result<(), DtcgError> {
        if depth > self.limits.max_reference_depth {
            Err(DtcgError::LimitExceeded {
                kind: "reference depth",
                limit: self.limits.max_reference_depth,
            })
        } else {
            Ok(())
        }
    }

    /// Counts one expansion step against `max_documents`, so reference fan-out
    /// is bounded even when the expanded sets contribute no source.
    fn count_expansion(&mut self) -> Result<(), DtcgError> {
        self.expansions += 1;
        self.check_count(self.expansions)
    }

    fn check_count(&self, count: usize) -> Result<(), DtcgError> {
        if count > self.limits.max_documents {
            Err(DtcgError::LimitExceeded {
                kind: "documents",
                limit: self.limits.max_documents,
            })
        } else {
            Ok(())
        }
    }
}

/// Whether a local source reference points into `resolutionOrder` or
/// `modifiers`, judged on the decoded pointer so every spelling is caught.
fn targets_forbidden_member(reference: &str) -> Result<bool, DtcgError> {
    let parts = pointer_parts(reference)?;
    Ok(parts.len() > 1 && matches!(parts[0].as_str(), "resolutionOrder" | "modifiers"))
}
