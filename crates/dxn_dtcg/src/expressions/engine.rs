//! One QuickJS session per resolving call: created at the first expression
//! token and dropped when the call returns, so no state carries over
//! between calls. A session compiles each token's expression into a
//! generated ES module whose default export takes the inputs, calls it with
//! the frozen inputs and converts the result to JSON.

use std::{cell::RefCell, collections::HashMap, fmt::Write, rc::Rc};

use rquickjs::{
    CatchResultExt, CaughtError, Context, Ctx, Error as JsError, Function, Module, Object,
    Persistent, Runtime, Value as JsValue, context::intrinsic, function::Args, loader,
};
use serde_json::{Number, Value};

use super::{
    ExpressionOptions,
    compile::{CompileError, Compiled, Position, check_expression, compile, position_at},
    format::{Expression, Import},
    sandbox::{CountingAllocator, Guard, HARDEN, Trip},
};
use crate::token_resolution::MAX_VALUE_DEPTH;

/// Why an evaluation failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Failure {
    /// A syntax error, an uncaught exception or a result that is not JSON.
    Message(String),
    /// A step, memory or stack limit.
    Limit(Trip),
    /// The result nests deeper than [`MAX_VALUE_DEPTH`].
    ValueDepth,
    /// The result has more nodes than the resolution may still copy.
    ValueNodes,
}

/// A compiled expression: its default export, and how to map positions in
/// its generated module back to the authored expression.
struct CompiledExpression {
    name: String,
    function: Persistent<Function<'static>>,
    compiled: Compiled,
    /// Where the expression's text sits in the generated module.
    span: Span,
}

/// The lines of the generated module before the expression's text, and the
/// position just past the text's end.
#[derive(Clone, Copy, Debug)]
struct Span {
    header_lines: u32,
    end: Position,
}

impl Span {
    /// The authored position of a position in the generated module; a
    /// position past the text (in the wrapper's closing lines) is its end.
    fn authored(self, position: Position) -> Option<Position> {
        let line = position.line.checked_sub(self.header_lines)?;
        if line == 0 {
            return None;
        }
        if line > self.end.line {
            return Some(self.end);
        }
        Some(Position {
            line,
            column: position.column,
        })
    }
}

/// The modules a session can import, by the name QuickJS knows them by.
#[derive(Default)]
struct ModuleTable {
    sources: HashMap<String, String>,
}

struct TableResolver(Rc<RefCell<ModuleTable>>);
struct TableLoader(Rc<RefCell<ModuleTable>>);

impl loader::Resolver for TableResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<loader::ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        if self.0.borrow().sources.contains_key(name) {
            Ok(name.to_owned())
        } else {
            Err(JsError::new_resolving_message(
                base,
                name,
                format!("unknown module `{name}`"),
            ))
        }
    }
}

impl loader::Loader for TableLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<loader::ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, rquickjs::module::Declared>> {
        let source = self
            .0
            .borrow()
            .sources
            .get(name)
            .cloned()
            .ok_or_else(|| JsError::new_loading_message(name, "unknown module"))?;
        Module::declare(ctx.clone(), name, source)
    }
}

pub(crate) struct Session {
    // Persistent handles are released before the context and runtime.
    expressions: HashMap<String, Rc<CompiledExpression>>,
    deep_freeze: Persistent<Function<'static>>,
    drain: Persistent<Function<'static>>,
    object_prototype: Persistent<Object<'static>>,
    context: Context,
    _runtime: Runtime,
    _modules: Rc<RefCell<ModuleTable>>,
    guard: Rc<Guard>,
    precision: Option<u32>,
}

/// The smallest stack limit QuickJS is given: below it, it cannot even
/// start an evaluation.
const MIN_STACK_LIMIT: usize = 16 * 1024;

impl Session {
    /// Starts QuickJS with the sandbox of `options`.
    pub(crate) fn new(options: &ExpressionOptions) -> Result<Self, Failure> {
        let stack_limit = options.stack_limit.max(MIN_STACK_LIMIT);
        // An evaluation without top-level code meets at most one interrupt
        // check, so a budget of 1 lets it through whatever ran before.
        let step_budget = options.step_budget.max(1);
        let guard = Guard::new(options.memory_limit, step_budget, stack_limit);
        let failed = |guard: &Guard, error: String| {
            guard
                .tripped()
                .map_or(Failure::Message(error), Failure::Limit)
        };
        let runtime = Runtime::new_with_alloc(CountingAllocator::new(Rc::clone(&guard)))
            .map_err(|error| failed(&guard, error.to_string()))?;
        runtime.set_max_stack_size(stack_limit);
        runtime.set_interrupt_handler(Some(guard.interrupt_handler()));
        let modules = Rc::new(RefCell::new(ModuleTable::default()));
        runtime.set_loader(
            TableResolver(Rc::clone(&modules)),
            TableLoader(Rc::clone(&modules)),
        );
        let context = Context::custom::<(
            intrinsic::Eval,
            intrinsic::Json,
            intrinsic::RegExpCompiler,
            intrinsic::RegExp,
            intrinsic::MapSet,
            intrinsic::TypedArrays,
            intrinsic::Promise,
        )>(&runtime)
        .map_err(|error| failed(&guard, error.to_string()))?;
        let (deep_freeze, drain, object_prototype) = context
            .with(|ctx| {
                let object_prototype = ctx
                    .globals()
                    .get::<_, Object>("Object")?
                    .get::<_, Object>("prototype")?;
                // Promises exist only because module evaluation needs them:
                // expressions cannot create or await any.
                ctx.globals().remove("Promise")?;
                let helpers: rquickjs::Array = ctx.eval(HARDEN)?;
                let deep_freeze: Function = helpers.get(0)?;
                let drain: Function = helpers.get(1)?;
                Ok::<_, JsError>((
                    Persistent::save(&ctx, deep_freeze),
                    Persistent::save(&ctx, drain),
                    Persistent::save(&ctx, object_prototype),
                ))
            })
            .map_err(|error| failed(&guard, error.to_string()))?;
        guard.enforce().map_err(Failure::Limit)?;
        Ok(Self {
            expressions: HashMap::new(),
            deep_freeze,
            drain,
            object_prototype,
            context,
            _runtime: runtime,
            _modules: modules,
            guard,
            precision: options.precision,
        })
    }

    /// Evaluates `expression` of the token at `path` with the resolved
    /// `inputs` (in the order of `expression.inputs`). The result may have
    /// at most `max_nodes` JSON nodes.
    ///
    /// The step budget counts QuickJS interrupt checks, which QuickJS makes
    /// after every 10,000 function calls and loop iterations, counted per
    /// context and carried across evaluations: an evaluation may meet its
    /// first check early. An evaluation that exceeds its budget therefore
    /// runs again after a full count, so whether it fits never depends on
    /// what ran before it.
    pub(crate) fn evaluate(
        &mut self,
        path: &str,
        expression: &Expression,
        inputs: &[Value],
        max_nodes: usize,
    ) -> Result<Value, Failure> {
        if let Some(trip) = self.guard.tripped() {
            return Err(Failure::Limit(trip));
        }
        let (source, span) = wrapper(expression);
        let compiled = match self.expressions.get(&source) {
            Some(compiled) => Rc::clone(compiled),
            None => {
                let compiled = self.compile_expression(path, &source, expression, span)?;
                self.expressions.insert(source, Rc::clone(&compiled));
                compiled
            }
        };
        match self.call(&compiled, inputs, max_nodes) {
            Err(Failure::Limit(Trip::Steps)) => {
                self.drain_counter();
                self.call(&compiled, inputs, max_nodes)
            }
            result => result,
        }
    }

    /// Calls the default export of `compiled` with the frozen `inputs` and
    /// converts its result.
    fn call(
        &self,
        compiled: &CompiledExpression,
        inputs: &[Value],
        max_nodes: usize,
    ) -> Result<Value, Failure> {
        let precision = self.precision;
        self.guard.begin();
        let result = self.context.with(|ctx| {
            let function = compiled.function.clone().restore(&ctx).catch(&ctx);
            let deep_freeze = self.deep_freeze.clone().restore(&ctx).catch(&ctx);
            let object_prototype = self.object_prototype.clone().restore(&ctx).catch(&ctx);
            let (function, deep_freeze, object_prototype) =
                match (function, deep_freeze, object_prototype) {
                    (Ok(function), Ok(deep_freeze), Ok(object_prototype)) => {
                        (function, deep_freeze, object_prototype)
                    }
                    (Err(error), ..) | (_, Err(error), _) | (.., Err(error)) => {
                        return Err(Failure::Message(caught_message(&error, None)));
                    }
                };
            let mut args = Args::new(ctx.clone(), inputs.len());
            for input in inputs {
                let value = ctx
                    .json_parse(input.to_string())
                    .and_then(|value| deep_freeze.call::<_, JsValue>((value,)))
                    .and_then(|value| args.push_arg(value))
                    .catch(&ctx);
                if let Err(error) = value {
                    return Err(Failure::Message(caught_message(&error, None)));
                }
            }
            let value = function
                .call_arg::<JsValue>(args)
                .catch(&ctx)
                .map_err(|error| {
                    Failure::Message(caught_message(&error, Some(compiled.locator())))
                })?;
            let mut converter = Converter {
                ctx: &ctx,
                object_prototype: &object_prototype,
                precision,
                nodes: 0,
                max_nodes,
            };
            let mut location = String::new();
            converter.convert(&value, 1, &mut location)
        });
        match self.guard.end() {
            Some(trip) => Err(Failure::Limit(trip)),
            None => result,
        }
    }

    /// Runs an empty loop until QuickJS's next interrupt check, which
    /// restarts its count of calls and iterations from the full 10,000.
    fn drain_counter(&self) {
        self.guard.drain();
        self.context.with(|ctx| {
            if let Ok(drain) = self.drain.clone().restore(&ctx) {
                let _ = drain.call::<_, JsValue>(());
            }
            let _ = ctx.catch();
        });
    }

    /// Compiles and evaluates the generated module of an expression and
    /// keeps its default export.
    fn compile_expression(
        &mut self,
        path: &str,
        source: &str,
        expression: &Expression,
        span: Span,
    ) -> Result<Rc<CompiledExpression>, Failure> {
        check_expression(&expression.source, expression.language).map_err(|error| {
            Failure::Message(match error.position {
                Some(position) => format!("{} {}", error.message, at(position)),
                None => error.message,
            })
        })?;
        let compiled = compile(source, expression.language)
            .map_err(|error| Failure::Message(authored_error(&error, span)))?;
        let name = format!("dxn.dtcg/expression/{path}");
        self.guard.begin();
        let function = self.context.with(|ctx| {
            let locator = Locator {
                name: &name,
                compiled: &compiled,
                span,
            };
            let function = Module::declare(ctx.clone(), name.as_str(), compiled.code.as_str())
                .and_then(Module::eval)
                .and_then(|(module, promise)| {
                    // An expression module has no top-level code, so its
                    // evaluation settles at once.
                    if let Some(Err(error)) = promise.result::<JsValue>() {
                        return Err(error);
                    }
                    module.get::<_, Function>("default")
                })
                .catch(&ctx)
                .map_err(|error| Failure::Message(caught_message(&error, Some(locator))))?;
            Ok(Persistent::save(&ctx, function))
        });
        if let Some(trip) = self.guard.end() {
            return Err(Failure::Limit(trip));
        }
        Ok(Rc::new(CompiledExpression {
            name,
            function: function?,
            compiled,
            span,
        }))
    }
}

impl CompiledExpression {
    fn locator(&self) -> Locator<'_> {
        Locator {
            name: &self.name,
            compiled: &self.compiled,
            span: self.span,
        }
    }
}

/// How to find the authored position of a stack frame in a generated
/// expression module.
#[derive(Clone, Copy)]
struct Locator<'a> {
    name: &'a str,
    compiled: &'a Compiled,
    span: Span,
}

/// The generated module of an expression and where the expression's text
/// sits in it:
///
/// ```js
/// import { a } from "module";
/// export default function (input) {
/// return (
/// <expression>
/// );
/// }
/// ```
fn wrapper(expression: &Expression) -> (String, Span) {
    let mut source = String::new();
    for (specifier, import) in &expression.imports {
        let specifier = Value::String(specifier.clone());
        match import {
            Import::Names(names) => {
                let _ = writeln!(
                    source,
                    "import {{ {} }} from {specifier};",
                    names.join(", ")
                );
            }
            Import::Namespace(name) => {
                let _ = writeln!(source, "import * as {name} from {specifier};");
            }
        }
    }
    let inputs = expression
        .inputs
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let _ = write!(source, "export default function ({inputs}) {{\nreturn (\n");
    let span = Span {
        header_lines: u32::try_from(source.matches('\n').count()).unwrap_or(u32::MAX),
        end: position_at(&expression.source, expression.source.len()),
    };
    source.push_str(&expression.source);
    source.push_str("\n);\n}\n");
    (source, span)
}

/// A compile error with its position in the authored expression.
fn authored_error(error: &CompileError, span: Span) -> String {
    match error.position.and_then(|position| span.authored(position)) {
        Some(position) => format!("{} {}", error.message, at(position)),
        None => error.message.clone(),
    }
}

fn at(position: Position) -> String {
    format!("at line {}, column {}", position.line, position.column)
}

/// The message of a caught exception, with the authored position of the
/// innermost stack frame in the expression `locator` finds, when known.
fn caught_message(error: &CaughtError<'_>, locator: Option<Locator<'_>>) -> String {
    match error {
        CaughtError::Exception(exception) => {
            let name = exception
                .get::<_, Option<String>>("name")
                .ok()
                .flatten()
                .unwrap_or_else(|| "Error".into());
            let message = exception.message().unwrap_or_default();
            let mut text = if message.is_empty() {
                name
            } else {
                format!("{name}: {message}")
            };
            if let Some(position) = locator
                .and_then(|locator| frame_position(&exception.stack().unwrap_or_default(), locator))
            {
                let _ = write!(text, " {}", at(position));
            }
            text
        }
        CaughtError::Value(value) => {
            let shown = value
                .as_string()
                .and_then(|text| text.to_string().ok())
                .or_else(|| {
                    value
                        .ctx()
                        .json_stringify(value.clone())
                        .ok()
                        .flatten()
                        .and_then(|text| text.to_string().ok())
                })
                .unwrap_or_else(|| format!("{:?}", value.type_of()));
            format!("uncaught exception: {shown}")
        }
        CaughtError::Error(error) => error.to_string(),
    }
}

/// The authored position of the innermost stack frame in the generated
/// module `locator` names: frames read `    at function (module:line:column)`,
/// and those of syntax errors `    at module:line:column`.
fn frame_position(stack: &str, locator: Locator<'_>) -> Option<Position> {
    let framed = format!("({}:", locator.name);
    let bare = format!("at {}:", locator.name);
    stack.lines().find_map(|line| {
        let location = line
            .find(&framed)
            .map(|index| &line[index + framed.len()..])
            .or_else(|| line.find(&bare).map(|index| &line[index + bare.len()..]))?;
        let location = location.strip_suffix(')').unwrap_or(location);
        let (line, column) = location.split_once(':')?;
        let generated = Position {
            line: line.parse().ok()?,
            column: column.parse().ok()?,
        };
        locator
            .span
            .authored(locator.compiled.source_position(generated))
    })
}

/// Converts an expression result to JSON: `null`, booleans, strings,
/// numbers (integral ones as integers), arrays and plain objects.
struct Converter<'a, 'js> {
    ctx: &'a Ctx<'js>,
    object_prototype: &'a Object<'js>,
    precision: Option<u32>,
    nodes: usize,
    max_nodes: usize,
}

impl<'js> Converter<'_, 'js> {
    fn convert(
        &mut self,
        value: &JsValue<'js>,
        depth: usize,
        location: &mut String,
    ) -> Result<Value, Failure> {
        if depth > MAX_VALUE_DEPTH {
            return Err(Failure::ValueDepth);
        }
        self.nodes += 1;
        if self.nodes > self.max_nodes {
            return Err(Failure::ValueNodes);
        }
        let invalid = |what: &str, location: &str| {
            let at = if location.is_empty() {
                "the result".to_owned()
            } else {
                format!("the result at `{location}`")
            };
            Failure::Message(format!("{at} is {what}, not a JSON value"))
        };
        if value.is_null() {
            return Ok(Value::Null);
        }
        if value.is_undefined() {
            return Err(invalid("undefined", location));
        }
        if let Some(boolean) = value.as_bool() {
            return Ok(Value::Bool(boolean));
        }
        if let Some(integer) = value.as_int() {
            return Ok(Value::from(integer));
        }
        if let Some(float) = value.as_float() {
            return number(float, self.precision).ok_or_else(|| {
                invalid(if float.is_nan() { "NaN" } else { "an infinity" }, location)
            });
        }
        if let Some(text) = value.as_string() {
            return text
                .to_string()
                .map(Value::String)
                .map_err(|error| Failure::Message(error.to_string()));
        }
        if value.is_symbol() {
            return Err(invalid("a symbol", location));
        }
        if value.is_big_int() {
            return Err(invalid("a bigint", location));
        }
        if value.is_function() {
            return Err(invalid("a function", location));
        }
        let length = location.len();
        if let Some(array) = value.as_array() {
            let mut items = Vec::with_capacity(array.len().min(self.max_nodes));
            for (index, item) in array.iter::<JsValue>().enumerate() {
                let item = item
                    .catch(self.ctx)
                    .map_err(|error| Failure::Message(caught_message(&error, None)))?;
                let _ = write!(location, "[{index}]");
                items.push(self.convert(&item, depth + 1, location)?);
                location.truncate(length);
            }
            return Ok(Value::Array(items));
        }
        let Some(object) = value.as_object() else {
            return Err(invalid(&format!("{:?}", value.type_of()), location));
        };
        let plain = object
            .get_prototype()
            .is_none_or(|prototype| prototype.as_value() == self.object_prototype.as_value());
        if !plain {
            return Err(invalid("an object with a prototype", location));
        }
        let mut members = serde_json::Map::new();
        for entry in object.props::<String, JsValue>() {
            let (key, item) = entry
                .catch(self.ctx)
                .map_err(|error| Failure::Message(caught_message(&error, None)))?;
            if !location.is_empty() {
                location.push('.');
            }
            location.push_str(&key);
            let item = self.convert(&item, depth + 1, location)?;
            location.truncate(length);
            members.insert(key, item);
        }
        Ok(Value::Object(members))
    }
}

/// A finite number as JSON, rounded to `precision` decimal places unless it
/// is integral; integral numbers within ±2^53 become integers.
fn number(value: f64, precision: Option<u32>) -> Option<Value> {
    if !value.is_finite() {
        return None;
    }
    let value = match precision {
        Some(places) if value.fract() != 0.0 => {
            let places = usize::try_from(places).unwrap_or(usize::MAX).min(64);
            format!("{value:.places$}").parse().unwrap_or(value)
        }
        _ => value,
    };
    const SAFE: f64 = 9_007_199_254_740_992.0;
    if value.fract() == 0.0 && value.abs() <= SAFE {
        #[expect(clippy::cast_possible_truncation, reason = "integral and within ±2^53")]
        return Some(Value::from(value as i64));
    }
    Number::from_f64(value).map(Value::Number)
}
