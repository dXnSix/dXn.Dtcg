//! Source checks and TypeScript stripping with oxc, before QuickJS sees any
//! code: syntax errors with their authored positions, the rejection of
//! dynamic `import()`, the static import specifiers of a module, and the
//! JavaScript of TypeScript sources with a map back to their positions.

use std::{collections::BTreeSet, fmt::Write, path::Path};

use oxc::{
    allocator::Allocator,
    ast::ast::{
        CatchClause, ExportAllDeclaration, ExportFromDeclaration, ImportDeclaration,
        ImportExpression, TryStatement,
    },
    ast_visit::{Visit, walk},
    codegen::{Codegen, CodegenOptions},
    diagnostics::OxcDiagnostic,
    parser::{ParseOptions, Parser},
    semantic::SemanticBuilder,
    span::SourceType,
    transformer::{TransformOptions, Transformer, TypeScriptOptions},
};

use super::format::{Language, check_identifier};

/// The internal module whose `freezeAll` the generated end of every module
/// calls.
pub(crate) const FREEZE_MODULE: &str = "dxn.dtcg/freeze";

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
    /// The module's own top-level bindings, imports excluded, in source
    /// order.
    top_level: Vec<String>,
    /// Every name the top level binds or references, so generated code can
    /// pick names that collide with none.
    root_names: BTreeSet<String>,
    /// The text [`instrument`] inserted into the JavaScript: line,
    /// character column where it starts, and length.
    insertions: Vec<(u32, u32, u32)>,
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
    /// The JavaScript of a module imported as `own_specifier`, followed by
    /// code that deep-freezes its namespace and top-level bindings once its
    /// top-level code has run: tokens share a module's values within a
    /// resolution, so none of them can change what another sees.
    pub(crate) fn module_code(&self, own_specifier: &str) -> String {
        let local = |base: &str| {
            let mut name = base.to_owned();
            let mut suffix = 0;
            while self.root_names.contains(&name) {
                suffix += 1;
                name = format!("{base}{suffix}");
            }
            name
        };
        let namespace = local("__dxn_dtcg_self");
        let freeze = local("__dxn_dtcg_freeze");
        let mut code = self.code.clone();
        let quote = |text: &str| serde_json::Value::from(text).to_string();
        let _ = write!(
            code,
            "\n;import * as {namespace} from {};\nimport {{ freezeAll as {freeze} }} from {};\n{freeze}({namespace}",
            quote(own_specifier),
            quote(FREEZE_MODULE),
        );
        for name in &self.top_level {
            let _ = write!(code, ", {name}");
        }
        code.push_str(");\n");
        code
    }

    /// The source position of a position in the generated JavaScript: the
    /// same position for JavaScript, the nearest preceding mapped segment
    /// for TypeScript.
    pub(crate) fn source_position(&self, generated: Position) -> Position {
        let generated = self.uninstrumented(generated);
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

impl Compiled {
    /// The character column of a 1-based UTF-8 byte column of `line` in the
    /// code, as QuickJS reports columns.
    pub(crate) fn char_column(&self, line: u32, byte_column: u32) -> u32 {
        let Some(text) = self.code.lines().nth((line as usize).saturating_sub(1)) else {
            return byte_column;
        };
        let mut end = (byte_column as usize).saturating_sub(1).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text[..end].chars().count() as u32 + 1
    }

    /// A position in the instrumented JavaScript as a position in the
    /// JavaScript before [`instrument`]: inserted text never spans lines,
    /// so only columns after an insertion on the same line move.
    fn uninstrumented(&self, generated: Position) -> Position {
        let mut shift = 0;
        for &(line, start, length) in &self.insertions {
            if line != generated.line || generated.column < start {
                continue;
            }
            if generated.column < start + length {
                return Position {
                    line,
                    column: start - shift,
                };
            }
            shift += length;
        }
        Position {
            line: generated.line,
            column: generated.column - shift,
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
    if let Some(offset) = scan.internal {
        return Err(CompileError {
            message: format!("unknown module `{FREEZE_MODULE}`"),
            position: Some(position_at(source, offset)),
        });
    }
    if let Some(offset) = scan.dynamic {
        return Err(CompileError {
            message: "dynamic `import()` is not available in expressions; use static imports"
                .into(),
            position: Some(position_at(source, offset)),
        });
    }
    if language == Language::JavaScript {
        let mut tries = TryScan::default();
        tries.visit_program(&program);
        let scoping = semantic.semantic.scoping();
        let root = scoping.root_scope_id();
        let mut top_level = Vec::new();
        let mut root_names = BTreeSet::new();
        for (name, symbol) in scoping.get_bindings(root) {
            let name = name.as_str();
            root_names.insert(name.to_owned());
            if !scoping.symbol_flags(*symbol).is_import() && check_identifier(name).is_ok() {
                top_level.push((scoping.symbol_span(*symbol).start, name.to_owned()));
            }
        }
        root_names.extend(
            scoping
                .root_unresolved_references()
                .keys()
                .map(|name| name.as_str().to_owned()),
        );
        top_level.sort();
        let mut taken = root_names.clone();
        taken.extend(scoping.symbol_names().map(str::to_owned));
        let comments = program
            .comments
            .iter()
            .map(|comment| (comment.span.start as usize, comment.span.end as usize))
            .collect::<Vec<_>>();
        let (code, insertions, check) = instrument(source, &tries.statements, &comments, &taken);
        root_names.extend(check);
        return Ok(Compiled {
            code,
            imports: scan.specifiers,
            top_level: top_level.into_iter().map(|(_, name)| name).collect(),
            root_names,
            insertions,
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
    let javascript = compile_unguarded(&generated.code, Language::JavaScript)?;
    Ok(Compiled {
        code: javascript.code,
        imports: javascript.imports,
        top_level: javascript.top_level,
        root_names: javascript.root_names,
        insertions: javascript.insertions,
        map: Some(map),
    })
}

/// A `try` statement's offsets: where it starts, its `catch` clause's
/// start and end, and where its `finally` block starts.
struct Try {
    start: usize,
    catch: Option<(usize, usize)>,
    finally: Option<usize>,
}

#[derive(Default)]
struct TryScan {
    statements: Vec<Try>,
}

impl<'a> Visit<'a> for TryScan {
    fn visit_try_statement(&mut self, it: &TryStatement<'a>) {
        self.statements.push(Try {
            start: it.span.start as usize,
            catch: it.handler.as_deref().map(|handler: &CatchClause<'a>| {
                (handler.span.start as usize, handler.span.end as usize)
            }),
            finally: it.finalizer.as_ref().map(|block| block.span.start as usize),
        });
        walk::walk_try_statement(self, it);
    }
}

/// Makes stack overflows uncatchable. QuickJS reports one as an ordinary
/// `RangeError` that `catch` could swallow, and a `finally` block that
/// returns would discard, so every caught value first goes to the internal
/// `check`, which stops the evaluation on a stack overflow:
///
/// - `catch (e) { B }` becomes
///   `catch (x) { check(x); try { throw x; } catch (e) { B } }`, which
///   keeps any parameter pattern and `catch` without one;
/// - a `try` with `finally` gets a `catch` that checks and rethrows before
///   the `finally` block runs: `try { A } finally { C }` becomes
///   `try { A } catch (x) { check(x); throw x; } finally { C }`, and
///   `try { A } catch (e) { B } finally { C }` becomes
///   `try { try { A } catch (e) { B } } catch (x) { ... } finally { C }`.
///
/// Only text is inserted, never removed or moved, and never a line break,
/// so positions map back by columns. Returns the code, the insertions and
/// the name `check` is imported as, if the source has a `try`.
fn instrument(
    source: &str,
    tries: &[Try],
    comments: &[(usize, usize)],
    taken: &BTreeSet<String>,
) -> (String, Vec<(u32, u32, u32)>, Option<String>) {
    if tries.is_empty() {
        return (source.to_owned(), Vec::new(), None);
    }
    let fresh = |base: &str| {
        let mut name = base.to_owned();
        let mut suffix = 0;
        while taken.contains(&name) {
            suffix += 1;
            name = format!("{base}{suffix}");
        }
        name
    };
    let check = fresh("__dxn_dtcg_check");
    let caught = fresh("__dxn_dtcg_caught");
    // Insertions at one offset: closing text before opening text.
    let mut edits: Vec<(usize, u8, String)> = Vec::new();
    for statement in tries {
        if let Some((start, end)) = statement.catch {
            edits.push((
                start + "catch".len(),
                1,
                format!(" ({caught}) {{ {check}({caught}); try {{ throw {caught}; }} catch"),
            ));
            edits.push((end, 0, " }".to_owned()));
        }
        if let Some(block) = statement.finally {
            let keyword = finally_keyword(source, statement.start, block, comments);
            let rethrow = format!("catch ({caught}) {{ {check}({caught}); throw {caught}; }} ");
            if statement.catch.is_some() {
                // The `try` and its `catch` move into an outer `try`.
                edits.push((statement.start, 1, "try { ".to_owned()));
                edits.push((keyword, 0, format!("}} {rethrow}")));
            } else {
                edits.push((keyword, 0, rethrow));
            }
        }
    }
    edits.sort_by_key(|(offset, order, _)| (*offset, *order));
    let mut code = String::with_capacity(source.len() + edits.len() * 48);
    let mut offsets = Vec::with_capacity(edits.len());
    let mut copied = 0;
    for (offset, _, text) in &edits {
        code.push_str(&source[copied..*offset]);
        copied = *offset;
        offsets.push((code.len(), text.len()));
        code.push_str(text);
    }
    code.push_str(&source[copied..]);
    let insertions = offsets
        .into_iter()
        .map(|(offset, length)| {
            let position = position_at(&code, offset);
            (position.line, position.column, length as u32)
        })
        .collect();
    let _ = write!(
        code,
        "\n;import {{ check as {check} }} from {};\n",
        serde_json::Value::from(FREEZE_MODULE)
    );
    (code, insertions, Some(check))
}

/// The offset of the `finally` keyword before the block at `block`: the
/// last occurrence after `start` outside comments.
fn finally_keyword(source: &str, start: usize, block: usize, comments: &[(usize, usize)]) -> usize {
    let mut end = block;
    while let Some(found) = source[start..end].rfind("finally") {
        let offset = start + found;
        if !comments
            .iter()
            .any(|(from, to)| (*from..*to).contains(&offset))
        {
            return offset;
        }
        end = offset;
    }
    block
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
    /// Where source text imports the internal module, which only generated
    /// code may.
    internal: Option<usize>,
}

impl ImportScan {
    fn add(&mut self, specifier: &str, offset: u32) {
        if specifier == FREEZE_MODULE {
            self.internal.get_or_insert(offset as usize);
        }
        self.specifiers.push(specifier.to_owned());
    }
}

impl<'a> Visit<'a> for ImportScan {
    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        self.add(&it.source.value, it.source.span.start);
    }

    fn visit_export_from_declaration(&mut self, it: &ExportFromDeclaration<'a>) {
        self.add(&it.source.value, it.source.span.start);
    }

    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        self.add(&it.source.value, it.source.span.start);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        self.dynamic.get_or_insert(it.span.start as usize);
        walk::walk_import_expression(self, it);
    }
}
