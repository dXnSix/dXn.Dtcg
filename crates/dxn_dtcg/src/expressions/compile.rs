//! Source checks and TypeScript stripping with oxc, before QuickJS sees any
//! code: syntax errors with their authored positions, the rejection of
//! dynamic `import()`, the static import specifiers of a module, and the
//! JavaScript of TypeScript sources with a map back to their positions.

use std::path::Path;

use oxc::{
    allocator::Allocator,
    ast::ast::{ExportAllDeclaration, ExportFromDeclaration, ImportDeclaration, ImportExpression},
    ast_visit::{Visit, walk},
    codegen::{Codegen, CodegenOptions},
    diagnostics::OxcDiagnostic,
    parser::{ParseOptions, Parser},
    semantic::SemanticBuilder,
    span::SourceType,
    transformer::{TransformOptions, Transformer, TypeScriptOptions},
};

use super::format::Language;

/// A position in a source: 1-based line and column (in characters).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Position {
    pub(crate) line: u32,
    pub(crate) column: u32,
}

/// Why a source does not compile: a message and where it applies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CompileError {
    pub(crate) message: String,
    pub(crate) position: Option<Position>,
}

/// The JavaScript of a module and, for TypeScript, the map from its
/// positions back to the source.
#[derive(Debug)]
pub(crate) struct Compiled {
    pub(crate) code: String,
    /// The static import and export-from specifiers, in source order.
    pub(crate) imports: Vec<String>,
    map: Option<Vec<Mapping>>,
}

/// One source map segment, 0-based: generated line and column, source line
/// and column.
#[derive(Clone, Copy, Debug)]
struct Mapping {
    generated: (u32, u32),
    source: (u32, u32),
}

impl Compiled {
    /// The source position of a position in the generated JavaScript: the
    /// same position for JavaScript, the nearest preceding mapped segment
    /// for TypeScript.
    pub(crate) fn source_position(&self, generated: Position) -> Position {
        let Some(map) = &self.map else {
            return generated;
        };
        let target = (generated.line - 1, generated.column.saturating_sub(1));
        let index = map.partition_point(|mapping| mapping.generated <= target);
        match index.checked_sub(1).map(|index| map[index]) {
            Some(mapping) if mapping.generated.0 == target.0 => Position {
                line: mapping.source.0 + 1,
                column: mapping.source.1 + 1 + (target.1 - mapping.generated.1),
            },
            Some(mapping) => Position {
                line: mapping.source.0 + 1,
                column: mapping.source.1 + 1,
            },
            None => Position { line: 1, column: 1 },
        }
    }
}

/// Checks that an authored `js` or `ts` expression is exactly one
/// expression, so its text cannot reach outside the generated module that
/// wraps it, and reports its syntax errors at their authored positions.
pub(crate) fn check_expression(source: &str, language: Language) -> Result<(), CompileError> {
    std::panic::catch_unwind(|| {
        super::nesting::check(source)?;
        let allocator = Allocator::default();
        let source_type = match language {
            Language::JavaScript => SourceType::mjs(),
            Language::TypeScript => SourceType::ts(),
        };
        Parser::new(&allocator, source, source_type)
            .with_options(parse_options())
            .parse_expression()
            .map(|_| ())
            .map_err(|errors| {
                errors.into_iter().next().map_or_else(
                    || CompileError {
                        message: "SyntaxError: invalid expression".into(),
                        position: None,
                    },
                    |error| diagnostic(source, &error),
                )
            })
    })
    .unwrap_or_else(|_| {
        Err(CompileError {
            message: "the expression could not be compiled".into(),
            position: None,
        })
    })
}

/// Parses `source` as an ES module, rejects dynamic `import()`, collects its
/// static imports and, for TypeScript, removes the types. A panic inside
/// oxc is reported as an error of the source rather than unwinding into the
/// caller.
pub(crate) fn compile(source: &str, language: Language) -> Result<Compiled, CompileError> {
    std::panic::catch_unwind(|| compile_unguarded(source, language)).unwrap_or_else(|_| {
        Err(CompileError {
            message: "the source could not be compiled".into(),
            position: None,
        })
    })
}

fn compile_unguarded(source: &str, language: Language) -> Result<Compiled, CompileError> {
    let allocator = Allocator::default();
    let source_type = match language {
        Language::JavaScript => SourceType::mjs(),
        Language::TypeScript => SourceType::ts(),
    };
    super::nesting::check(source)?;
    let parsed = Parser::new(&allocator, source, source_type)
        .with_options(parse_options())
        .parse();
    if let Some(error) = parsed.diagnostics.into_iter().next() {
        return Err(diagnostic(source, &error));
    }
    let mut program = parsed.program;
    // The early errors only semantic analysis finds: redeclarations,
    // octal literals in strict code, duplicate regular expression groups.
    let semantic = SemanticBuilder::new()
        .with_check_syntax_error(true)
        .with_enum_eval(true)
        .build(&program);
    if let Some(error) = semantic.diagnostics.errors().next() {
        return Err(diagnostic(source, error));
    }
    let mut scan = ImportScan::default();
    scan.visit_program(&program);
    if let Some(offset) = scan.dynamic {
        return Err(CompileError {
            message: "dynamic `import()` is not available in expressions; use static imports"
                .into(),
            position: Some(position_at(source, offset)),
        });
    }
    if language == Language::JavaScript {
        return Ok(Compiled {
            code: source.to_owned(),
            imports: scan.specifiers,
            map: None,
        });
    }
    let scoping = semantic.semantic.into_scoping();
    let options = TransformOptions {
        // Keep every import a module declares, as JavaScript does, so an
        // unused import still loads (and fails) the same way.
        typescript: TypeScriptOptions {
            only_remove_type_imports: true,
            ..TypeScriptOptions::default()
        },
        ..TransformOptions::default()
    };
    let transformed = Transformer::new(&allocator, Path::new("expression.ts"), &options)
        .build_with_scoping(scoping, &mut program);
    if let Some(error) = transformed.diagnostics.into_iter().next() {
        return Err(diagnostic(source, &error));
    }
    let generated = Codegen::new()
        .with_options(CodegenOptions {
            source_map_path: Some("expression.ts".into()),
            ..CodegenOptions::default()
        })
        .build(&program);
    let mut map = generated
        .map
        .map(|map| {
            map.get_tokens()
                .map(|token| Mapping {
                    generated: (token.get_dst_line(), token.get_dst_col()),
                    source: (token.get_src_line(), token.get_src_col()),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    map.sort_by_key(|mapping| mapping.generated);
    // A type-only import disappears with its types: only the remaining
    // imports load.
    let imports = compile_unguarded(&generated.code, Language::JavaScript)
        .map(|compiled| compiled.imports)
        .unwrap_or(scan.specifiers);
    Ok(Compiled {
        code: generated.code,
        imports,
        map: Some(map),
    })
}

/// Regular expression literals are validated too, so their errors have
/// positions (QuickJS reports none).
fn parse_options() -> ParseOptions {
    ParseOptions {
        parse_regular_expression: true,
        ..ParseOptions::default()
    }
}

fn diagnostic(source: &str, error: &OxcDiagnostic) -> CompileError {
    CompileError {
        message: format!("SyntaxError: {}", error.message),
        // The primary label marks the offending text; a redeclaration also
        // labels the first declaration.
        position: error
            .labels
            .iter()
            .find(|label| label.primary())
            .or(error.labels.last())
            .map(|label| position_at(source, label.offset() as usize)),
    }
}

/// The 1-based line and character column of a byte offset.
pub(crate) fn position_at(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let before = &source[..source.floor_char_boundary(offset)];
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    Position {
        line: u32::try_from(before.matches('\n').count() + 1).unwrap_or(u32::MAX),
        column: u32::try_from(before[line_start..].chars().count() + 1).unwrap_or(u32::MAX),
    }
}

/// The static import specifiers of a module and the first dynamic
/// `import()`.
#[derive(Default)]
struct ImportScan {
    specifiers: Vec<String>,
    dynamic: Option<usize>,
}

impl<'a> Visit<'a> for ImportScan {
    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        self.specifiers.push(it.source.value.to_string());
    }

    fn visit_export_from_declaration(&mut self, it: &ExportFromDeclaration<'a>) {
        self.specifiers.push(it.source.value.to_string());
    }

    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        self.specifiers.push(it.source.value.to_string());
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        self.dynamic.get_or_insert(it.span.start as usize);
        walk::walk_import_expression(self, it);
    }
}
