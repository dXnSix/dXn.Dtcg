//! Bounded resolution of token aliases and JSON Pointer references.
//!
//! One hop counter per resolution chain covers token aliases, token-target
//! pointers and document pointers, and each hop counts toward
//! `RuntimeLimits::max_alias_depth`. The check fires before descending, and
//! literal values cost no hops. Resolved tokens and document-pointer targets
//! are memoized together with their hop depth, JSON node count and nesting
//! depth, so a chain resolves to the same result in any authored order and
//! repeated references are resolved once. Every value a reference copies is
//! charged against `max_value_nodes` before it is copied, so duplicated
//! references cannot build exponentially large values.
//!
//! Nesting adds up across references: a pointer to a nested value, placed
//! inside another nested value, nests deeper than either. Every node's
//! absolute depth in the token value being built is therefore checked against
//! [`MAX_VALUE_DEPTH`], `serde_json`'s parse recursion limit, before the
//! resolver descends into it or copies a resolved value there. Recursion is
//! thus bounded by the value depth plus the hop limit, and every resolved value
//! can be cloned, dropped, validated, hashed and serialized safely.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde_json::{Map, Value};

use crate::{
    DtcgError, ResolvedToken, ResourceId, RuntimeLimits, RuntimeOptions, ValueExtensions,
    flatten::RawToken,
    json_pointer::{pointer_parts, walk},
    reference_positions::{Context, Position, Step},
    validate_value_with,
};

/// The deepest nesting of a resolved value, root level included: the
/// recursion limit `serde_json` applies when parsing documents.
pub(crate) const MAX_VALUE_DEPTH: usize = 128;

/// A document pointer's identity: the document it is evaluated in (by
/// address) and its unescaped reference tokens.
type PointerKey = (usize, Vec<String>);

/// The reference hops below a value, its JSON node count and its nesting
/// depth (1 for a scalar).
#[derive(Clone, Copy, Debug, Default)]
struct Cost {
    hops: usize,
    nodes: usize,
    depth: usize,
}

pub(crate) struct Resolver<'a> {
    raw: &'a BTreeMap<String, RawToken>,
    limits: &'a RuntimeLimits,
    extensions: &'a ValueExtensions,
    resolved: BTreeMap<String, ResolvedToken>,
    costs: HashMap<String, Cost>,
    visiting: Vec<String>,
    active_pointers: HashSet<PointerKey>,
    pointers: HashMap<PointerKey, (Value, Cost)>,
    stack_hops: usize,
    copied_nodes: usize,
    #[cfg(feature = "expressions")]
    expressions: Option<&'a crate::expressions::ExpressionOptions>,
    /// The JavaScript engine of this resolution, started at the first
    /// expression token.
    #[cfg(feature = "expressions")]
    session: Option<crate::expressions::Session>,
    /// The document modules expressions may import.
    #[cfg(feature = "expressions")]
    document_modules: Option<&'a crate::expressions::DocumentModules>,
}

impl<'a> Resolver<'a> {
    pub(crate) fn new(raw: &'a BTreeMap<String, RawToken>, options: &'a RuntimeOptions) -> Self {
        Self {
            raw,
            limits: &options.limits,
            extensions: &options.extensions,
            resolved: BTreeMap::new(),
            costs: HashMap::new(),
            visiting: Vec::new(),
            active_pointers: HashSet::new(),
            pointers: HashMap::new(),
            stack_hops: 0,
            copied_nodes: 0,
            #[cfg(feature = "expressions")]
            expressions: options.expressions.as_ref(),
            #[cfg(feature = "expressions")]
            session: None,
            #[cfg(feature = "expressions")]
            document_modules: None,
        }
    }

    /// Lets expressions import `modules`.
    #[cfg(feature = "expressions")]
    pub(crate) fn with_document_modules(
        &mut self,
        modules: &'a crate::expressions::DocumentModules,
    ) {
        self.document_modules = Some(modules);
    }

    pub(crate) fn into_resolved(self) -> BTreeMap<String, ResolvedToken> {
        self.resolved
    }

    /// Resolves and validates the token at `path` (a key of `raw`).
    pub(crate) fn resolve_token(&mut self, path: &str) -> Result<(), DtcgError> {
        self.token_cost(path, 1).map(|_| ())
    }

    /// Resolves the token at `path`, whose value is placed at absolute depth
    /// `depth` of the value being built.
    fn token_cost(&mut self, path: &str, depth: usize) -> Result<Cost, DtcgError> {
        if let Some(cost) = self.costs.get(path) {
            return Ok(*cost);
        }
        let raw = self.raw;
        let token = raw
            .get(path)
            .ok_or_else(|| self.unresolved(path, "missing alias target"))?;
        if self.visiting.iter().any(|item| item == path) {
            return Err(DtcgError::Token {
                resource: token.source.clone(),
                path: path.into(),
                message: format!("alias cycle: {} -> {path}", self.visiting.join(" -> ")),
            });
        }
        // Members of a value that is not a whole reference follow the token's
        // own type, or else its group's.
        let position = token
            .own_type
            .as_deref()
            .or(token.group_type.as_deref())
            .map_or(Position::Unchecked, |token_type| {
                Position::Root(Context::of_type(token_type))
            });
        self.visiting.push(path.into());
        let result = self.resolve_value(&token.value, token.document.as_ref(), depth, position);
        self.visiting.pop();
        let (value, cost) = result?;
        // Format §5.2.2: own `$type`, then the type of the token a whole-value
        // reference targets, then the nearest group's `$type`.
        let reference_type = whole_token_reference(&token.value, raw)?
            .and_then(|reference| self.resolved.get(&reference))
            .and_then(|target| target.token_type.clone());
        if let (Some(own), Some(found)) = (&token.own_type, &reference_type)
            && own != found
        {
            return Err(DtcgError::TokenType {
                resource: token.source.clone(),
                path: path.into(),
                message: format!("reference type mismatch: expected {own}, found {found}"),
            });
        }
        let token_type = token
            .own_type
            .clone()
            .or(reference_type)
            .or_else(|| token.group_type.clone())
            .ok_or_else(|| DtcgError::TokenType {
                resource: token.source.clone(),
                path: path.into(),
                message: "token requires a DTCG type".into(),
            })?;
        validate_value_with(&token_type, &value, self.extensions).map_err(|error| {
            DtcgError::TokenType {
                resource: token.source.clone(),
                path: path.into(),
                message: error.to_string(),
            }
        })?;
        #[cfg(feature = "expressions")]
        let (value, snapshot, cost) =
            self.evaluate_expression(path, token, &token_type, value, cost, depth)?;
        #[cfg(not(feature = "expressions"))]
        let snapshot = None;
        self.resolved.insert(
            path.into(),
            ResolvedToken {
                path: path.into(),
                token_type: Some(token_type),
                authored_value: token.authored_value.clone(),
                value,
                snapshot,
                extensions: token.extensions.clone(),
                description: token.description.clone(),
                deprecated: token.deprecated.clone(),
                source: token.source.clone(),
            },
        );
        self.costs.insert(path.into(), cost);
        Ok(cost)
    }

    /// With expressions enabled and an expression on `token`, resolves its
    /// inputs, evaluates it and returns its validated result with the
    /// resolved `$value` as the snapshot and the result's cost; otherwise
    /// returns `value` and `cost` unchanged.
    #[cfg(feature = "expressions")]
    fn evaluate_expression(
        &mut self,
        path: &str,
        token: &RawToken,
        token_type: &str,
        value: Value,
        cost: Cost,
        depth: usize,
    ) -> Result<(Value, Option<Value>, Cost), DtcgError> {
        use crate::expressions::{Failure, Trip, token_expression};

        let Some(options) = self.expressions else {
            return Ok((value, None, cost));
        };
        let Some(expression) = token_expression(&token.source, path, &token.extensions)? else {
            return Ok((value, None, cost));
        };
        // Inputs resolve like a token value at an unchecked position, with
        // the token on the visiting stack, so an input that reaches the
        // token is an alias cycle.
        let mut hops = cost.hops;
        let mut inputs = Vec::with_capacity(expression.inputs.len());
        self.visiting.push(path.into());
        let mut resolved = Ok(());
        for (_, input) in &expression.inputs {
            match self.resolve_value(input, token.document.as_ref(), 1, Position::Unchecked) {
                Ok((input, input_cost)) => {
                    hops = hops.max(input_cost.hops);
                    inputs.push(input);
                }
                Err(error) => {
                    resolved = Err(error);
                    break;
                }
            }
        }
        self.visiting.pop();
        resolved?;
        let failed = |failure: Failure| match failure {
            Failure::Message(message) => DtcgError::Expression {
                resource: token.source.clone(),
                path: path.into(),
                message,
            },
            // A document module names itself and the specifier it was
            // imported by; a host module, the token that imported it.
            Failure::Module {
                resource: Some(resource),
                specifier,
                message,
            } => DtcgError::Expression {
                resource,
                path: specifier,
                message,
            },
            Failure::Module {
                resource: None,
                specifier,
                message,
            } => DtcgError::Expression {
                resource: token.source.clone(),
                path: path.into(),
                message: format!("module `{specifier}`: {message}"),
            },
            Failure::Limit(Trip::Steps) => DtcgError::LimitExceeded {
                kind: "expression steps",
                limit: options.step_budget.max(1),
            },
            Failure::Limit(Trip::Memory) => DtcgError::LimitExceeded {
                kind: "expression memory",
                limit: options.memory_limit,
            },
            Failure::Limit(Trip::Stack) => DtcgError::Expression {
                resource: token.source.clone(),
                path: path.into(),
                message: format!(
                    "stack overflow: the evaluation exceeded the stack limit of {} bytes",
                    options.stack_limit
                ),
            },
            Failure::ValueDepth => DtcgError::LimitExceeded {
                kind: "value depth",
                limit: MAX_VALUE_DEPTH,
            },
            Failure::ValueNodes => DtcgError::LimitExceeded {
                kind: "value nodes",
                limit: self.limits.max_value_nodes,
            },
        };
        let session = match &mut self.session {
            Some(session) => session,
            None => self.session.insert(
                crate::expressions::Session::new(options, self.document_modules, self.extensions)
                    .map_err(&failed)?,
            ),
        };
        let max_nodes = self
            .limits
            .max_value_nodes
            .saturating_sub(self.copied_nodes);
        let result = session
            .evaluate(path, &expression, &inputs, max_nodes)
            .map_err(&failed)?;
        let (nodes, result_depth) = (node_count(&result), value_depth(&result));
        check_depth(depth + result_depth - 1)?;
        self.charge(nodes)?;
        validate_value_with(token_type, &result, self.extensions).map_err(|error| {
            DtcgError::TokenType {
                resource: token.source.clone(),
                path: path.into(),
                message: format!("expression result: {error}"),
            }
        })?;
        Ok((
            result,
            Some(value),
            Cost {
                hops,
                nodes,
                depth: result_depth,
            },
        ))
    }

    /// Takes one reference hop to the token at `target`, whose value will be
    /// placed at absolute depth `depth`, resolving it first when needed.
    /// Returns the token's own cost.
    fn reference_token(&mut self, target: &str, depth: usize) -> Result<Cost, DtcgError> {
        if !self.raw.contains_key(target) {
            return Err(self.unresolved(target, "missing alias target"));
        }
        let cost = match self.costs.get(target) {
            Some(cost) => *cost,
            None => {
                self.enter_hop()?;
                let result = self.token_cost(target, depth);
                self.stack_hops -= 1;
                result?
            }
        };
        self.check_hops(cost.hops + 1)?;
        check_depth(depth + cost.depth - 1)?;
        Ok(cost)
    }

    fn enter_hop(&mut self) -> Result<(), DtcgError> {
        self.check_hops(1)?;
        self.stack_hops += 1;
        Ok(())
    }

    /// Fails when a chain of `hops` more references below the current stack
    /// exceeds `max_alias_depth`.
    fn check_hops(&self, hops: usize) -> Result<(), DtcgError> {
        if self.stack_hops.saturating_add(hops) > self.limits.max_alias_depth {
            Err(DtcgError::LimitExceeded {
                kind: "alias depth",
                limit: self.limits.max_alias_depth,
            })
        } else {
            Ok(())
        }
    }

    /// Charges a copy of `nodes` JSON nodes against `max_value_nodes`. Called
    /// before every copy of a resolved value into the value being built.
    fn charge(&mut self, nodes: usize) -> Result<(), DtcgError> {
        let total = self.copied_nodes.saturating_add(nodes);
        if total > self.limits.max_value_nodes {
            return Err(DtcgError::LimitExceeded {
                kind: "value nodes",
                limit: self.limits.max_value_nodes,
            });
        }
        self.copied_nodes = total;
        Ok(())
    }

    /// Resolves `value`, placed at absolute depth `depth` (1 for a token's
    /// whole value) and at `position` of the value being built.
    fn resolve_value(
        &mut self,
        value: &Value,
        document: &Value,
        depth: usize,
        position: Position,
    ) -> Result<(Value, Cost), DtcgError> {
        check_depth(depth)?;
        if let Some(reference) = curly_reference(value) {
            if let Position::Inside(_) = position {
                return Err(self.misplaced_alias());
            }
            let cost = self.reference_token(reference, depth)?;
            if let Position::Member { expected, .. } = position {
                self.check_member_type(reference, expected)?;
            }
            self.charge(cost.nodes)?;
            let value = self.resolved[reference].value.clone();
            return Ok((
                value,
                Cost {
                    hops: cost.hops + 1,
                    ..cost
                },
            ));
        }
        match value {
            Value::Object(object) if object.contains_key("$ref") => {
                self.resolve_json_reference(object, document, depth, position)
            }
            Value::Array(values) => {
                let mut cost = Cost {
                    hops: 0,
                    nodes: 1,
                    depth: 1,
                };
                let mut output = Vec::with_capacity(values.len());
                let element = position.child(Step::Element, self.extensions.line_height_dimension);
                for item in values {
                    let (item, item_cost) =
                        self.resolve_value(item, document, depth + 1, element)?;
                    cost = combine(cost, item_cost);
                    output.push(item);
                }
                Ok((Value::Array(output), cost))
            }
            Value::Object(values) => {
                let mut cost = Cost {
                    hops: 0,
                    nodes: 1,
                    depth: 1,
                };
                let mut output = Map::with_capacity(values.len());
                for (key, item) in values {
                    let member =
                        position.child(Step::Member(key), self.extensions.line_height_dimension);
                    let (item, item_cost) =
                        self.resolve_value(item, document, depth + 1, member)?;
                    cost = combine(cost, item_cost);
                    output.insert(key.clone(), item);
                }
                Ok((Value::Object(output), cost))
            }
            scalar => Ok((
                scalar.clone(),
                Cost {
                    hops: 0,
                    nodes: 1,
                    depth: 1,
                },
            )),
        }
    }

    /// Resolves a `{ "$ref": … }` object at absolute depth `depth`: the
    /// pointer target, then its sibling overrides. The pointer is no longer
    /// active while the overrides resolve, so an override may point at the
    /// same target.
    fn resolve_json_reference(
        &mut self,
        object: &Map<String, Value>,
        document: &Value,
        depth: usize,
        position: Position,
    ) -> Result<(Value, Cost), DtcgError> {
        let reference = object
            .get("$ref")
            .and_then(Value::as_str)
            .ok_or_else(|| DtcgError::InvalidPointer("$ref must be a string".into()))?;
        let parts = pointer_parts(reference)?;
        let (mut value, mut cost) = match token_target(&parts, self.raw) {
            Some((marker, token_path)) => {
                self.resolve_token_pointer(reference, &parts, marker, &token_path, depth)?
            }
            None => self.resolve_document_pointer(reference, parts, document, depth)?,
        };
        if object.len() == 1 {
            return Ok((value, cost));
        }
        let target = value
            .as_object_mut()
            .ok_or_else(|| DtcgError::InvalidPointer(reference.into()))?;
        for (key, item) in object {
            if key != "$ref" {
                let member =
                    position.child(Step::Member(key), self.extensions.line_height_dimension);
                let (item, item_cost) = self.resolve_value(item, document, depth + 1, member)?;
                cost = combine(cost, item_cost);
                target.insert(key.clone(), item);
            }
        }
        Ok((value, cost))
    }

    /// A pointer into a token's `$value` or `$type`, such as
    /// `#/colors/red/$value/components/0`.
    fn resolve_token_pointer(
        &mut self,
        reference: &str,
        parts: &[String],
        marker: usize,
        token_path: &str,
        depth: usize,
    ) -> Result<(Value, Cost), DtcgError> {
        let cost = self.reference_token(token_path, depth)?;
        let selected = if parts[marker] == "$value" {
            let token = &self.resolved[token_path];
            let selected = walk(&token.value, &parts[marker + 1..])
                .ok_or_else(|| DtcgError::InvalidPointer(reference.into()))?;
            let (nodes, selected_depth) = (node_count(selected), value_depth(selected));
            self.charge(nodes)?;
            let selected = walk(&self.resolved[token_path].value, &parts[marker + 1..])
                .expect("the pointer was just resolved")
                .clone();
            (selected, nodes, selected_depth)
        } else {
            if marker + 1 != parts.len() {
                return Err(DtcgError::InvalidPointer(reference.into()));
            }
            let token_type = self.resolved[token_path]
                .token_type
                .clone()
                .ok_or_else(|| self.unresolved(reference, "JSON Pointer type target is missing"))?;
            self.charge(1)?;
            (Value::String(token_type.as_ref().into()), 1, 1)
        };
        let (selected, nodes, selected_depth) = selected;
        Ok((
            selected,
            Cost {
                hops: cost.hops + 1,
                nodes,
                depth: selected_depth,
            },
        ))
    }

    /// A pointer to any other location of the referring token's document,
    /// such as `#/$extensions/palette`, placed at absolute depth `depth`.
    /// Results are memoized per document and pointer, and the same identity
    /// detects cycles.
    fn resolve_document_pointer(
        &mut self,
        reference: &str,
        parts: Vec<String>,
        document: &Value,
        depth: usize,
    ) -> Result<(Value, Cost), DtcgError> {
        let key = (std::ptr::from_ref(document) as usize, parts);
        let cost = match self.pointers.get(&key) {
            Some((_, cost)) => *cost,
            None => {
                if self.active_pointers.contains(&key) {
                    return Err(DtcgError::InvalidPointer(format!(
                        "circular JSON Pointer reference `{reference}`"
                    )));
                }
                let target = walk(document, &key.1)
                    .ok_or_else(|| DtcgError::InvalidPointer(reference.into()))?;
                self.enter_hop()?;
                self.active_pointers.insert(key.clone());
                let result = self.resolve_value(target, document, depth, Position::Unchecked);
                self.active_pointers.remove(&key);
                self.stack_hops -= 1;
                let (value, cost) = result?;
                self.pointers.insert(key.clone(), (value, cost));
                cost
            }
        };
        self.check_hops(cost.hops + 1)?;
        check_depth(depth + cost.depth - 1)?;
        self.charge(cost.nodes)?;
        let value = self.pointers[&key].0.clone();
        Ok((
            value,
            Cost {
                hops: cost.hops + 1,
                ..cost
            },
        ))
    }

    /// A curly-brace alias at a position the Format module does not allow.
    fn misplaced_alias(&self) -> DtcgError {
        let path = self.visiting.last().cloned().unwrap_or_default();
        let token = &self.raw[&path];
        let token_type = token
            .own_type
            .as_deref()
            .or(token.group_type.as_deref())
            .unwrap_or_default();
        DtcgError::Token {
            resource: token.source.clone(),
            path,
            message: format!("curly-brace references are not allowed inside {token_type} values"),
        }
    }

    /// Fails unless the resolved token at `reference` has one of `expected`.
    fn check_member_type(&self, reference: &str, expected: &[&str]) -> Result<(), DtcgError> {
        let found = self.resolved[reference]
            .token_type
            .as_deref()
            .unwrap_or_default();
        if expected.contains(&found) {
            return Ok(());
        }
        let path = self.visiting.last().cloned().unwrap_or_default();
        Err(DtcgError::TokenType {
            resource: self.raw[&path].source.clone(),
            path,
            message: format!(
                "reference type mismatch: expected {}, found {found}",
                expected.join(" or ")
            ),
        })
    }

    /// An unresolved reference from the token being resolved (the innermost
    /// entry of `visiting`) to `target`.
    fn unresolved(&self, target: &str, message: &str) -> DtcgError {
        let path = self.visiting.last().cloned().unwrap_or_default();
        let resource = self
            .raw
            .get(&path)
            .map(|token| token.source.clone())
            .unwrap_or_else(|| ResourceId::parse("tokens").expect("static id"));
        DtcgError::UnresolvedReference {
            resource,
            path,
            target: target.into(),
            message: message.into(),
        }
    }
}

/// The cost of a composite value `left` after adding the member `right`.
fn combine(left: Cost, right: Cost) -> Cost {
    Cost {
        hops: left.hops.max(right.hops),
        nodes: left.nodes.saturating_add(right.nodes),
        depth: left.depth.max(right.depth + 1),
    }
}

/// Fails when a node would sit deeper than [`MAX_VALUE_DEPTH`].
fn check_depth(depth: usize) -> Result<(), DtcgError> {
    if depth > MAX_VALUE_DEPTH {
        Err(DtcgError::LimitExceeded {
            kind: "value depth",
            limit: MAX_VALUE_DEPTH,
        })
    } else {
        Ok(())
    }
}

/// The nesting depth of a resolved value, which is at most
/// [`MAX_VALUE_DEPTH`], so the recursion is bounded.
fn value_depth(value: &Value) -> usize {
    match value {
        Value::Array(values) => 1 + values.iter().map(value_depth).max().unwrap_or(0),
        Value::Object(values) => 1 + values.values().map(value_depth).max().unwrap_or(0),
        _ => 1,
    }
}

fn node_count(value: &Value) -> usize {
    match value {
        Value::Array(values) => values
            .iter()
            .fold(1, |count, value| count.saturating_add(node_count(value))),
        Value::Object(values) => values
            .values()
            .fold(1, |count, value| count.saturating_add(node_count(value))),
        _ => 1,
    }
}

/// The token path of a curly-brace alias string such as `{colors.red}`.
pub(crate) fn curly_reference(value: &Value) -> Option<&str> {
    value
        .as_str()?
        .strip_prefix('{')
        .and_then(|text| text.strip_suffix('}'))
}

/// The `$value`/`$type` marker index and token path when `parts` points into a
/// token of `raw`; any other pointer addresses the document.
fn token_target(parts: &[String], raw: &BTreeMap<String, RawToken>) -> Option<(usize, String)> {
    parts
        .iter()
        .position(|part| part == "$value" || part == "$type")
        .filter(|index| *index > 0)
        .map(|index| (index, parts[..index].join(".")))
        .filter(|(_, token_path)| raw.contains_key(token_path))
}

/// The target token path when a whole value is a reference: a curly-brace
/// alias, or a single-member `{ "$ref": "#/…/$value" }` (also the form of a
/// token-level `$ref`) whose prefix is a token. Any other pointer ending in
/// `$value` addresses the document.
pub(crate) fn whole_token_reference(
    value: &Value,
    raw: &BTreeMap<String, RawToken>,
) -> Result<Option<String>, DtcgError> {
    if let Some(reference) = curly_reference(value).filter(|path| !path.is_empty()) {
        return Ok(Some(reference.into()));
    }
    let Some(object) = value.as_object() else {
        return Ok(None);
    };
    if object.len() != 1 {
        return Ok(None);
    }
    let Some(reference) = object.get("$ref").and_then(Value::as_str) else {
        return Ok(None);
    };
    let parts = pointer_parts(reference)?;
    let Some(value_index) = parts.iter().position(|part| part == "$value") else {
        return Ok(None);
    };
    if value_index + 1 != parts.len() || value_index == 0 {
        return Ok(None);
    }
    let path = parts[..value_index].join(".");
    Ok(raw.contains_key(&path).then_some(path))
}

/// What a token's value reads: the token paths it references directly (also
/// missing ones), and whether it follows any document pointer.
#[derive(Clone, Debug, Default)]
pub(crate) struct Dependencies {
    pub(crate) tokens: BTreeSet<String>,
    pub(crate) reads_document: bool,
}

/// Collects the token paths a token's value reads directly, following
/// document pointers transitively. Document pointers use the same hop limit,
/// identity and memoization as resolution.
pub(crate) struct DependencyCollector<'a> {
    raw: &'a BTreeMap<String, RawToken>,
    limits: &'a RuntimeLimits,
    active: HashSet<PointerKey>,
    pointers: HashMap<PointerKey, Dependencies>,
}

impl<'a> DependencyCollector<'a> {
    pub(crate) fn new(raw: &'a BTreeMap<String, RawToken>, limits: &'a RuntimeLimits) -> Self {
        Self {
            raw,
            limits,
            active: HashSet::new(),
            pointers: HashMap::new(),
        }
    }

    pub(crate) fn collect(
        &mut self,
        value: &Value,
        document: &Value,
        dependencies: &mut Dependencies,
    ) -> Result<(), DtcgError> {
        self.collect_at(value, document, 1, dependencies)
    }

    /// Collects from `value` at absolute depth `depth`, which is bounded by
    /// [`MAX_VALUE_DEPTH`] as in resolution.
    fn collect_at(
        &mut self,
        value: &Value,
        document: &Value,
        depth: usize,
        dependencies: &mut Dependencies,
    ) -> Result<(), DtcgError> {
        check_depth(depth)?;
        if let Some(reference) = curly_reference(value) {
            dependencies.tokens.insert(reference.to_owned());
            return Ok(());
        }
        match value {
            Value::Object(object) if object.contains_key("$ref") => {
                let reference = object
                    .get("$ref")
                    .and_then(Value::as_str)
                    .ok_or_else(|| DtcgError::InvalidPointer("$ref must be a string".into()))?;
                let parts = pointer_parts(reference)?;
                if let Some((_, token_path)) = token_target(&parts, self.raw) {
                    dependencies.tokens.insert(token_path);
                } else {
                    self.collect_pointer(reference, parts, document, depth, dependencies)?;
                }
                for (key, item) in object {
                    if key != "$ref" {
                        self.collect_at(item, document, depth + 1, dependencies)?;
                    }
                }
            }
            Value::Array(values) => {
                for item in values {
                    self.collect_at(item, document, depth + 1, dependencies)?;
                }
            }
            Value::Object(values) => {
                for item in values.values() {
                    self.collect_at(item, document, depth + 1, dependencies)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn collect_pointer(
        &mut self,
        reference: &str,
        parts: Vec<String>,
        document: &Value,
        depth: usize,
        dependencies: &mut Dependencies,
    ) -> Result<(), DtcgError> {
        dependencies.reads_document = true;
        let key = (std::ptr::from_ref(document) as usize, parts);
        if let Some(found) = self.pointers.get(&key) {
            dependencies.tokens.extend(found.tokens.iter().cloned());
            return Ok(());
        }
        if self.active.contains(&key) {
            return Err(DtcgError::InvalidPointer(format!(
                "circular JSON Pointer reference `{reference}`"
            )));
        }
        if self.active.len() >= self.limits.max_alias_depth {
            return Err(DtcgError::LimitExceeded {
                kind: "alias depth",
                limit: self.limits.max_alias_depth,
            });
        }
        let target =
            walk(document, &key.1).ok_or_else(|| DtcgError::InvalidPointer(reference.into()))?;
        self.active.insert(key.clone());
        let mut found = Dependencies::default();
        let result = self.collect_at(target, document, depth, &mut found);
        self.active.remove(&key);
        result?;
        dependencies.tokens.extend(found.tokens.iter().cloned());
        self.pointers.insert(key, found);
        Ok(())
    }
}
