//! Source checks and TypeScript stripping with oxc, before QuickJS sees any
//! code: syntax errors with their authored positions, the rejection of
//! dynamic `import()`, the static import specifiers of a module, and the
//! JavaScript of TypeScript sources with a map back to their positions.

use std::{collections::BTreeSet, fmt::Write, path::Path};

use oxc::{
    allocator::Allocator,
    ast::ast::{
        ArrowFunctionBody, CatchClause, ExportAllDeclaration, ExportFromDeclaration, Expression,
        Function, ImportDeclaration, ImportExpression, MethodDefinition, MethodDefinitionKind,
        ObjectProperty, PropertyDefinition, PropertyKey, TryStatement,
    },
    ast_visit::{Visit, walk},
    codegen::{Codegen, CodegenOptions},
    diagnostics::OxcDiagnostic,
    parser::{ParseOptions, Parser},
    semantic::SemanticBuilder,
    span::GetSpan,
    span::SourceType,
    syntax::scope::ScopeId,
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
    /// resolution, so none of them can change a value another reads (state
    /// freezing cannot reach, such as a `Map`'s contents, stays shared).
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
        let mut instrumented = Scan::default();
        instrumented.visit_program(&program);
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
        // Parameters the body redeclares with `var` or a function: the body
        // moves into an arrow, which must receive them to share their
        // bindings.
        let passed = instrumented
            .returns
            .iter()
            .map(|body| match *body {
                ReturnBody::Block(_, _, Some(scope), (from, to)) => scoping
                    .get_bindings(scope)
                    .iter()
                    .filter(|(_, symbol)| {
                        let declared = scoping.symbol_span(**symbol);
                        (from..to).contains(&declared.start)
                            && !scoping.symbol_redeclarations(**symbol).is_empty()
                    })
                    .map(|(name, _)| name.as_str().to_owned())
                    .collect(),
                _ => Vec::new(),
            })
            .collect::<Vec<Vec<String>>>();
        let (code, insertions, check) =
            instrument(source, &instrumented, &passed, &comments, &taken);
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

/// A `try` statement's offsets: its span, its `catch` clause's span, and
/// where its `finally` block starts.
struct Try {
    start: usize,
    end: usize,
    catch: Option<(usize, usize)>,
    finally: Option<usize>,
}

/// The body of an iterator's `return` method: a block's span with its
/// braces (with the function's scope and the span of its parameters), or
/// an arrow's expression.
enum ReturnBody {
    Block(usize, usize, Option<ScopeId>, (u32, u32)),
    Expression(usize, usize),
}

/// The statements and methods [`instrument`] rewrites.
#[derive(Default)]
struct Scan {
    tries: Vec<Try>,
    returns: Vec<ReturnBody>,
}

impl Scan {
    /// Records `value` when it is the plain function or arrow of a property
    /// named `return`: what an iterator's closing calls.
    fn return_method(&mut self, key: &PropertyKey<'_>, value: &Expression<'_>) {
        if key.static_name().as_deref() != Some("return") {
            return;
        }
        match value {
            Expression::FunctionExpression(function) => self.function(function),
            Expression::ArrowFunctionExpression(arrow) if !arrow.r#async => {
                let span = arrow.body.span();
                self.returns.push(match arrow.body {
                    ArrowFunctionBody::FunctionBody(_) => ReturnBody::Block(
                        span.start as usize,
                        span.end as usize,
                        arrow.scope_id.get(),
                        (arrow.params.span.start, arrow.params.span.end),
                    ),
                    _ => ReturnBody::Expression(span.start as usize, span.end as usize),
                });
            }
            _ => {}
        }
    }

    fn function(&mut self, function: &Function<'_>) {
        if !function.r#async
            && !function.generator
            && let Some(body) = &function.body
        {
            self.returns.push(ReturnBody::Block(
                body.span.start as usize,
                body.span.end as usize,
                function.scope_id.get(),
                (function.params.span.start, function.params.span.end),
            ));
        }
    }
}

impl<'a> Visit<'a> for Scan {
    fn visit_try_statement(&mut self, it: &TryStatement<'a>) {
        self.tries.push(Try {
            start: it.span.start as usize,
            end: it.span.end as usize,
            catch: it.handler.as_deref().map(|handler: &CatchClause<'a>| {
                (handler.span.start as usize, handler.span.end as usize)
            }),
            finally: it.finalizer.as_ref().map(|block| block.span.start as usize),
        });
        walk::walk_try_statement(self, it);
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        self.return_method(&it.key, &it.value);
        walk::walk_object_property(self, it);
    }

    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        if it.kind == MethodDefinitionKind::Method
            && it.key.static_name().as_deref() == Some("return")
        {
            self.function(&it.value);
        }
        walk::walk_method_definition(self, it);
    }

    fn visit_property_definition(&mut self, it: &PropertyDefinition<'a>) {
        if let Some(value) = &it.value {
            self.return_method(&it.key, value);
        }
        walk::walk_property_definition(self, it);
    }
}

/// One text insertion: where, whether it opens or closes a construct, and
/// the span of that construct, which orders insertions at one offset.
struct Edit {
    offset: usize,
    opening: bool,
    start: usize,
    end: usize,
    text: String,
}

impl Edit {
    /// At one offset closing text comes first, innermost construct first;
    /// then opening text, outermost construct first.
    fn order(&self) -> (usize, bool, usize, usize) {
        if self.opening {
            (self.offset, true, self.start, usize::MAX - self.end)
        } else {
            (self.offset, false, usize::MAX - self.start, 0)
        }
    }
}

/// Makes stack overflows uncatchable. QuickJS reports one as an ordinary
/// error, and the guard sees it only when QuickJS happens to allocate past
/// the guard's threshold (small objects come from QuickJS's arenas), so
/// what could handle or discard the error first passes it to the internal
/// `check`, which stops the evaluation on a stack overflow:
///
/// - `catch (e) { B }` becomes
///   `catch (x) { check(x); try { throw x; } catch (e) { B } }`, which
///   keeps any parameter pattern and `catch` without one;
/// - a `try` with `finally` gets a `catch` that checks and rethrows before
///   the `finally` block runs: `try { A } finally { C }` becomes
///   `try { A } catch (x) { check(x); throw x; } finally { C }`, and
///   `try { A } catch (e) { B } finally { C }` becomes
///   `try { try { A } catch (e) { B } } catch (x) { ... } finally { C }`;
/// - the body of a plain `return` method or arrow (what closing an
///   iterator calls, discarding what it throws) becomes
///   `{ return checked(() => { ... }); }`, which checks before rethrowing.
///
/// Only text is inserted, never removed or moved, and never a line break,
/// so positions map back by columns. Returns the code, the insertions and
/// the name `check` is imported as, if the source needed it.
fn instrument(
    source: &str,
    scan: &Scan,
    passed: &[Vec<String>],
    comments: &[(usize, usize)],
    taken: &BTreeSet<String>,
) -> (String, Vec<(u32, u32, u32)>, Option<String>) {
    if scan.tries.is_empty() && scan.returns.is_empty() {
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
    let checked = fresh("__dxn_dtcg_checked");
    let caught = fresh("__dxn_dtcg_caught");
    let rethrow = format!("catch ({caught}) {{ {check}({caught}); throw {caught}; }}");
    let mut edits = Vec::new();
    let mut edit = |offset: usize, opening: bool, (start, end): (usize, usize), text: String| {
        edits.push(Edit {
            offset,
            opening,
            start,
            end,
            text,
        });
    };
    for statement in &scan.tries {
        if let Some((start, end)) = statement.catch {
            edit(
                start + "catch".len(),
                true,
                (start, end),
                format!(" ({caught}) {{ {check}({caught}); try {{ throw {caught}; }} catch"),
            );
            edit(end, false, (start, end), " }".to_owned());
        }
        if let Some(block) = statement.finally {
            let keyword = finally_keyword(source, statement.start, block, comments);
            let span = (statement.start, statement.end);
            if statement.catch.is_some() {
                // The `try` and its `catch` move into an outer `try`.
                edit(statement.start, true, span, "try { ".to_owned());
                edit(keyword, false, span, format!("}} {rethrow} "));
            } else {
                edit(keyword, false, span, format!("{rethrow} "));
            }
        }
    }
    // A loop or built-in that closes an iterator because of a throw
    // discards what its `return` method throws, so the method's body runs
    // through `checked` (an arrow keeps its `this`, `arguments` and scope).
    for (body, names) in scan.returns.iter().zip(passed) {
        match *body {
            // An empty body cannot throw.
            ReturnBody::Block(start, end, ..) if end - start <= 2 => {}
            ReturnBody::Block(start, end, ..) => {
                let names = names.join(", ");
                let arguments = if names.is_empty() {
                    String::new()
                } else {
                    format!(", {names}")
                };
                edit(
                    start + 1,
                    true,
                    (start, end),
                    format!(" return {checked}(({names}) => {{"),
                );
                edit(end - 1, false, (start, end), format!("}}{arguments}); "));
            }
            ReturnBody::Expression(start, end) => {
                edit(
                    start,
                    true,
                    (start, end),
                    format!("{{ return {checked}(() => ("),
                );
                edit(end, false, (start, end), ")); }".to_owned());
            }
        }
    }
    edits.sort_by_key(Edit::order);
    let mut code = String::with_capacity(source.len() + edits.len() * 48);
    let mut offsets = Vec::with_capacity(edits.len());
    let mut copied = 0;
    for edit in &edits {
        code.push_str(&source[copied..edit.offset]);
        copied = edit.offset;
        offsets.push((code.len(), edit.text.len()));
        code.push_str(&edit.text);
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
        "\n;import {{ check as {check}, checked as {checked} }} from {};\n",
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
