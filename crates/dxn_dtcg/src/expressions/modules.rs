//! JavaScript and TypeScript modules: host-registered sources, the module
//! declarations of token documents, and the request/provide walk over the
//! static import graph of document modules.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    rc::Rc,
    sync::Arc,
};

use serde_json::Value;

use super::{
    ExpressionOptions,
    compile::{Compiled, compile},
    format::{Language, is_relative},
};
use crate::{
    DocumentKind, DocumentRequest, DtcgError, EXPRESSION_EXTENSION, Resolution, ResourceId,
    RuntimeOptions, SourceOccurrence, catalog::DocumentCatalog, flatten::RawToken,
};

/// The source text of an ES module that expressions import, registered by
/// the host with
/// [`ExpressionOptions::with_module`](super::ExpressionOptions::with_module).
/// It may import other host modules, the modules documents declare and the
/// built-in modules by their bare specifiers; it cannot import relatively.
#[derive(Clone, Eq, PartialEq)]
pub struct ExpressionModule {
    pub(crate) language: Language,
    pub(crate) source: Arc<str>,
}

impl ExpressionModule {
    /// A JavaScript ES module.
    pub fn javascript(source: impl Into<Arc<str>>) -> Self {
        Self {
            language: Language::JavaScript,
            source: source.into(),
        }
    }

    /// A TypeScript ES module; its types are removed, not checked.
    pub fn typescript(source: impl Into<Arc<str>>) -> Self {
        Self {
            language: Language::TypeScript,
            source: source.into(),
        }
    }
}

/// Prints the language and the source length, never the source.
impl fmt::Debug for ExpressionModule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExpressionModule")
            .field("language", &self.language)
            .field("bytes", &self.source.len())
            .finish()
    }
}

/// A relative import specifier as a document reference: ES modules write
/// `./name.js`, document references have no `.` segments.
pub(crate) fn import_reference(specifier: &str) -> String {
    specifier
        .split('/')
        .filter(|segment| *segment != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether `specifier` is in the namespace reserved for built-in modules:
/// `dxn.dtcg` and `dxn.dtcg/…`.
pub(crate) fn is_reserved(specifier: &str) -> bool {
    specifier == "dxn.dtcg" || specifier.starts_with("dxn.dtcg/")
}

/// The language of a module document, from its extension.
pub(crate) fn language_of(resource: &ResourceId) -> Option<Language> {
    let name = resource.as_str();
    if name.ends_with(".ts") || name.ends_with(".mts") {
        Some(Language::TypeScript)
    } else if name.ends_with(".js") || name.ends_with(".mjs") {
        Some(Language::JavaScript)
    } else {
        None
    }
}

/// A token document's declaration of one module specifier.
#[derive(Clone, Debug)]
pub(crate) struct Declaration {
    pub(crate) module: ResourceId,
    pub(crate) document: ResourceId,
    pub(crate) reference: Arc<str>,
}

/// Module sources by resource id, with their language.
pub(crate) type ModuleSources = BTreeMap<ResourceId, (Language, Arc<str>)>;

/// The document modules a resolution imports: what each declared specifier
/// names and the source of every module its imports reach.
#[derive(Debug, Default)]
pub(crate) struct DocumentModules {
    pub(crate) specifiers: BTreeMap<String, ResourceId>,
    pub(crate) sources: ModuleSources,
    /// Every module the walk compiled, by the name a session gives it (a
    /// host module's specifier, `./` and a document module's resource id).
    pub(crate) compiled: BTreeMap<String, Rc<Compiled>>,
}

/// The module declarations of the token documents among `sources`, in
/// resolution order: `$extensions["dxn.dtcg.expressions"].modules` at a
/// document's root, with references resolved against that document.
pub(crate) fn declarations(
    sources: &[SourceOccurrence],
) -> Result<Vec<(String, Declaration)>, DtcgError> {
    let mut found = Vec::new();
    for source in sources {
        // Only a whole document has a root; fragments and inline sources
        // declare nothing.
        let Some(document) = source.origin.resource.as_ref() else {
            continue;
        };
        if !source.origin.pointer.is_empty() {
            continue;
        }
        let Some(extension) = source
            .value
            .get("$extensions")
            .and_then(|extensions| extensions.get(EXPRESSION_EXTENSION))
        else {
            continue;
        };
        let error = |path: &str, message: String| DtcgError::Expression {
            resource: document.clone(),
            path: path.into(),
            message,
        };
        let object = extension.as_object().ok_or_else(|| {
            error(
                "",
                format!("the root `{EXPRESSION_EXTENSION}` extension must be an object"),
            )
        })?;
        if let Some(member) = object.keys().find(|key| *key != "modules") {
            return Err(error(
                "",
                format!("unknown member `{member}`; expected `modules`"),
            ));
        }
        let Some(modules) = object.get("modules") else {
            continue;
        };
        let modules = modules
            .as_object()
            .ok_or_else(|| error("", "`modules` must be an object".into()))?;
        for (specifier, reference) in modules {
            let invalid = |message: &str| error(specifier, message.into());
            if specifier.is_empty() {
                return Err(invalid("a module specifier must not be empty"));
            }
            if is_relative(specifier) {
                return Err(invalid(
                    "a declared module specifier must be bare, not relative",
                ));
            }
            if is_reserved(specifier) {
                return Err(invalid(
                    "the `dxn.dtcg` namespace is reserved for built-in modules",
                ));
            }
            let Value::String(reference) = reference else {
                return Err(invalid("a module reference must be a string"));
            };
            let resolved = document.resolve(reference).map_err(|problem| {
                invalid(&format!(
                    "`{reference}` is not a valid relative reference: {problem}"
                ))
            })?;
            if resolved.fragment().is_some() {
                return Err(invalid(&format!(
                    "`{reference}` names a module document, which has no fragment"
                )));
            }
            if language_of(resolved.resource_id()).is_none() {
                return Err(invalid(&format!(
                    "`{reference}` must end in `.js`, `.mjs`, `.ts` or `.mts`"
                )));
            }
            found.push((
                specifier.clone(),
                Declaration {
                    module: resolved.resource_id().clone(),
                    document: document.clone(),
                    reference: reference.as_str().into(),
                },
            ));
        }
    }
    Ok(found)
}

/// One module to load: its resource, who references it and how.
struct Pending {
    module: ResourceId,
    from: ResourceId,
    reference: Arc<str>,
    specifier: String,
}

/// The document modules a resolution of `raw` imports, or the requests for
/// those the catalog lacks. Only what expressions reach counts: the
/// specifiers their `imports` name, and through the host modules among
/// them, the specifiers those import. Nothing is requested without
/// expressions, or when nothing reached is a document module.
pub(crate) fn document_modules(
    catalog: &DocumentCatalog,
    sources: &[SourceOccurrence],
    raw: &BTreeMap<String, RawToken>,
    options: &RuntimeOptions,
) -> Result<Resolution<DocumentModules>, DtcgError> {
    let Some(expressions) = &options.expressions else {
        return Ok(Resolution::Ready(DocumentModules::default()));
    };
    let mut specifiers = BTreeMap::<String, Declaration>::new();
    for (specifier, declaration) in declarations(sources)? {
        if host_registers(expressions, &specifier) {
            return Err(DtcgError::Expression {
                resource: declaration.document,
                path: specifier.clone(),
                message: format!(
                    "`{specifier}` is both registered by the host and declared by a document"
                ),
            });
        }
        specifiers.insert(specifier, declaration);
    }
    let mut imported = BTreeSet::<String>::new();
    for token in raw.values() {
        if let Some(imports) = token
            .extensions
            .get(EXPRESSION_EXTENSION)
            .and_then(|expression| expression.get("imports"))
            .and_then(Value::as_object)
        {
            imported.extend(imports.keys().cloned());
        }
    }
    let roots = imported.into_iter().map(Next::Specifier).collect();
    let walked = walk(catalog, roots, &specifiers, expressions, &options.limits)?;
    Ok(walked.map(|(sources, compiled)| DocumentModules {
        specifiers: specifiers
            .into_iter()
            .map(|(specifier, declaration)| (specifier, declaration.module))
            .collect(),
        sources,
        compiled,
    }))
}

/// Whether the host registers `specifier` itself.
fn host_registers(expressions: &ExpressionOptions, specifier: &str) -> bool {
    expressions.modules.contains_key(specifier)
        || expressions.native_modules.contains_key(specifier)
}

/// The requests for every module any document among `sources` declares, in
/// every context, and the modules their imports reach.
pub(crate) fn validate_modules(
    catalog: &DocumentCatalog,
    sources: &[SourceOccurrence],
    options: &RuntimeOptions,
) -> Result<Resolution<()>, DtcgError> {
    let Some(expressions) = &options.expressions else {
        return Ok(Resolution::Ready(()));
    };
    let declared = declarations(sources)?;
    let roots = declared
        .iter()
        .map(|(specifier, declaration)| Next::Module(pending(specifier, declaration)))
        .collect();
    // Within a context a later declaration wins; across contexts any may.
    let specifiers = declared.into_iter().collect::<BTreeMap<_, _>>();
    walk(catalog, roots, &specifiers, expressions, &options.limits).map(|walked| walked.map(|_| ()))
}

fn pending(specifier: &str, declaration: &Declaration) -> Pending {
    Pending {
        module: declaration.module.clone(),
        from: declaration.document.clone(),
        reference: Arc::clone(&declaration.reference),
        specifier: specifier.into(),
    }
}

/// A step of the walk: a bare specifier to follow, or a document module to
/// load.
enum Next {
    Specifier(String),
    Module(Pending),
}

/// What a walk found: the document modules' sources, and every module it
/// compiled by the name a session gives it.
type Walked = (ModuleSources, BTreeMap<String, Rc<Compiled>>);

/// Follows `roots` through the static imports they reach: a bare specifier
/// to the host module it names (whose own imports are followed) or to the
/// document module a declaration names; a document module's relative
/// imports resolve against it. Document modules the catalog lacks become
/// requests.
fn walk(
    catalog: &DocumentCatalog,
    roots: Vec<Next>,
    specifiers: &BTreeMap<String, Declaration>,
    expressions: &ExpressionOptions,
    limits: &crate::RuntimeLimits,
) -> Result<Resolution<Walked>, DtcgError> {
    let mut queue = VecDeque::from(roots);
    let mut sources = BTreeMap::new();
    let mut compiled = BTreeMap::new();
    let mut missing = BTreeMap::<ResourceId, DocumentRequest>::new();
    let mut seen_specifiers = BTreeSet::new();
    let mut seen_modules = BTreeSet::new();
    while let Some(next) = queue.pop_front() {
        let next = match next {
            Next::Specifier(specifier) => {
                if !seen_specifiers.insert(specifier.clone()) {
                    continue;
                }
                if let Some(module) = expressions.modules.get(&specifier) {
                    // A host module that does not compile fails when an
                    // expression evaluates it.
                    if let Ok(host) = compile(&module.source, module.language) {
                        queue.extend(
                            host.imports
                                .iter()
                                .filter(|import| !is_relative(import))
                                .cloned()
                                .map(Next::Specifier),
                        );
                        compiled.insert(specifier, Rc::new(host));
                    }
                } else if let Some(declaration) = specifiers.get(&specifier) {
                    queue.push_back(Next::Module(pending(&specifier, declaration)));
                }
                continue;
            }
            Next::Module(next) => next,
        };
        if !seen_modules.insert(next.module.clone()) {
            continue;
        }
        if seen_modules.len() > limits.max_documents {
            return Err(DtcgError::LimitExceeded {
                kind: "documents",
                limit: limits.max_documents,
            });
        }
        let error = |message: String| DtcgError::Expression {
            resource: next.module.clone(),
            path: next.specifier.clone(),
            message,
        };
        let Some(language) = language_of(&next.module) else {
            return Err(error(format!(
                "`{}` must end in `.js`, `.mjs`, `.ts` or `.mts`",
                next.reference
            )));
        };
        let source = match catalog.kind(&next.module) {
            None => {
                missing
                    .entry(next.module.clone())
                    .or_insert_with(|| DocumentRequest {
                        resource_id: next.module.clone(),
                        referenced_from: next.from.clone(),
                        original_reference: Arc::clone(&next.reference),
                        kind: DocumentKind::Module,
                    });
                continue;
            }
            Some(DocumentKind::Json) => {
                return Err(error(
                    "the catalog holds it as a JSON document, not a module".into(),
                ));
            }
            Some(_) => Arc::clone(catalog.module(&next.module).expect("a module document")),
        };
        let module = compile(&source, language).map_err(|problem| {
            error(match problem.position {
                Some(position) => format!(
                    "{} at line {}, column {}",
                    problem.message, position.line, position.column
                ),
                None => problem.message,
            })
        })?;
        for import in &module.imports {
            if is_relative(import) {
                let target = next
                    .module
                    .resolve(import_reference(import))
                    .map_err(|problem| {
                        error(format!(
                            "import `{import}` is not a valid relative reference: {problem}"
                        ))
                    })?;
                if target.fragment().is_some() {
                    return Err(error(format!("import `{import}` must not have a fragment")));
                }
                queue.push_back(Next::Module(Pending {
                    module: target.resource_id().clone(),
                    from: next.module.clone(),
                    reference: import.as_str().into(),
                    specifier: import.clone(),
                }));
            } else {
                queue.push_back(Next::Specifier(import.clone()));
            }
        }
        compiled.insert(format!("./{}", next.module), Rc::new(module));
        sources.insert(next.module.clone(), (language, source));
    }
    if missing.is_empty() {
        Ok(Resolution::Ready((sources, compiled)))
    } else {
        Ok(Resolution::Missing(missing.into_values().collect()))
    }
}
