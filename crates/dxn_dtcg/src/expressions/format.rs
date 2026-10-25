//! The token-level expression object (`$extensions["dxn.dtcg.expressions"]`)
//! and its validation, which runs only when expressions are enabled.

use serde_json::{Map, Value};

use crate::{DtcgError, EXPRESSION_EXTENSION, ResourceId};

/// The language an expression or module is written in.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Language {
    JavaScript,
    TypeScript,
}

/// What a token imports from one module: named exports or the namespace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Import {
    Names(Vec<String>),
    Namespace(String),
}

/// A validated token expression.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Expression {
    pub(crate) language: Language,
    pub(crate) source: String,
    /// Input names and their authored values, in authored order.
    pub(crate) inputs: Vec<(String, Value)>,
    /// Module specifiers and what the expression imports from them, in
    /// authored order.
    pub(crate) imports: Vec<(String, Import)>,
}

/// The token's expression, when its `$extensions` declare one.
pub(crate) fn token_expression(
    resource: &ResourceId,
    path: &str,
    extensions: &Value,
) -> Result<Option<Expression>, DtcgError> {
    let Some(object) = extensions.get(EXPRESSION_EXTENSION) else {
        return Ok(None);
    };
    let error = |message: String| DtcgError::Expression {
        resource: resource.clone(),
        path: path.into(),
        message,
    };
    let object = object
        .as_object()
        .ok_or_else(|| error(format!("`{EXPRESSION_EXTENSION}` must be an object")))?;
    if let Some(member) = object
        .keys()
        .find(|key| !matches!(key.as_str(), "js" | "ts" | "inputs" | "imports"))
    {
        return Err(error(format!("unknown expression member `{member}`")));
    }
    let (language, source) = match (object.get("js"), object.get("ts")) {
        (Some(_), Some(_)) => {
            return Err(error(
                "an expression has either `js` or `ts`, not both".into(),
            ));
        }
        (Some(source), None) => (Language::JavaScript, ("js", source)),
        (None, Some(source)) => (Language::TypeScript, ("ts", source)),
        (None, None) => return Err(error("an expression needs `js` or `ts`".into())),
    };
    let (member, source) = source;
    let source = source
        .as_str()
        .ok_or_else(|| error(format!("`{member}` must be a string")))?
        .to_owned();
    let mut names = Vec::<String>::new();
    let mut declare = |name: &str, member: &str| -> Result<(), DtcgError> {
        check_identifier(name).map_err(|problem| error(format!("{member}: `{name}` {problem}")))?;
        if names.iter().any(|existing| existing == name) {
            return Err(error(format!("{member}: duplicate local name `{name}`")));
        }
        names.push(name.to_owned());
        Ok(())
    };
    let inputs = match object.get("inputs") {
        None => Vec::new(),
        Some(Value::Object(inputs)) => {
            for name in inputs.keys() {
                declare(name, "`inputs`")?;
            }
            inputs
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect()
        }
        Some(_) => return Err(error("`inputs` must be an object".into())),
    };
    let imports = match object.get("imports") {
        None => Vec::new(),
        Some(Value::Object(imports)) => parse_imports(imports, &error, &mut declare)?,
        Some(_) => return Err(error("`imports` must be an object".into())),
    };
    Ok(Some(Expression {
        language,
        source,
        inputs,
        imports,
    }))
}

fn parse_imports(
    imports: &Map<String, Value>,
    error: &impl Fn(String) -> DtcgError,
    declare: &mut impl FnMut(&str, &str) -> Result<(), DtcgError>,
) -> Result<Vec<(String, Import)>, DtcgError> {
    let mut parsed = Vec::with_capacity(imports.len());
    for (specifier, imported) in imports {
        if specifier.is_empty() {
            return Err(error(
                "`imports`: a module specifier must not be empty".into(),
            ));
        }
        if is_relative(specifier) {
            return Err(error(format!(
                "`imports`: `{specifier}` is relative; tokens import modules by bare specifier"
            )));
        }
        let member = format!("`imports` `{specifier}`");
        let import = match imported {
            Value::String(namespace) => {
                declare(namespace, &member)?;
                Import::Namespace(namespace.clone())
            }
            Value::Array(names) => {
                let mut imported = Vec::with_capacity(names.len());
                for name in names {
                    let name = name.as_str().ok_or_else(|| {
                        error(format!("{member}: imported names must be strings"))
                    })?;
                    declare(name, &member)?;
                    imported.push(name.to_owned());
                }
                Import::Names(imported)
            }
            _ => {
                return Err(error(format!(
                    "{member} must be an array of export names or a namespace name"
                )));
            }
        };
        parsed.push((specifier.clone(), import));
    }
    Ok(parsed)
}

/// Whether a module specifier is relative or absolute rather than bare.
pub(crate) fn is_relative(specifier: &str) -> bool {
    specifier.starts_with("./") || specifier.starts_with("../") || specifier.starts_with('/')
}

/// Words that cannot name a binding in an ES module (strict mode code),
/// including `eval` and `arguments`.
const RESERVED: &[&str] = &[
    "arguments",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "eval",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// Checks that `name` can be a local binding: an ECMAScript identifier
/// name (ID_Start or `$`/`_`, then ID_Continue, `$`, ZWNJ or ZWJ), written
/// without escapes, that is not a reserved word.
pub(crate) fn check_identifier(name: &str) -> Result<(), &'static str> {
    use oxc::syntax::identifier::{is_identifier_part, is_identifier_start};

    let mut chars = name.chars();
    let valid = chars.next().is_some_and(is_identifier_start) && chars.all(is_identifier_part);
    if !valid {
        Err("is not a JavaScript identifier")
    } else if RESERVED.contains(&name) {
        Err("is a reserved word")
    } else {
        Ok(())
    }
}
