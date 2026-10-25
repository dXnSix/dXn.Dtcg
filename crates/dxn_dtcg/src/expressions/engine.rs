//! One QuickJS session per resolving call: created at the first expression
//! token and dropped when the call returns, so no state carries over
//! between calls. A session compiles each token's expression into a
//! generated ES module whose default export takes the inputs, calls it with
//! the frozen inputs and converts the result to JSON.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fmt::Write,
    rc::Rc,
    sync::Arc,
};

use rquickjs::{
    CatchResultExt, CaughtError, Context, Ctx, Error as JsError, Function, Module, Object,
    Persistent, Runtime, Value as JsValue, context::intrinsic, function::Args, loader,
};
use serde_json::{Number, Value};
use sha2::{Digest, Sha256};

use super::{
    ExpressionOptions, builtins,
    compile::{
        CompileError, Compiled, FREEZE_MODULE, Position, check_expression, compile, position_at,
    },
    format::{Expression, Import},
    format::{Language, is_relative},
    modules::{DocumentModules, import_reference, is_reserved},
    native::{Export, NativeError, NativeFn, NativeModule},
    sandbox::{CountingAllocator, Guard, HARDEN, Trip},
};
use crate::{ResourceId, ValueExtensions, token_resolution::MAX_VALUE_DEPTH};

/// The modules a session can import: the host's and the documents'.
fn module_table(
    options: &ExpressionOptions,
    documents: Option<&DocumentModules>,
    natives: &[NativeModule],
) -> Result<ModuleTable, Failure> {
    let mut table = ModuleTable::default();
    for (index, module) in natives.iter().enumerate() {
        let specifier = module.specifier();
        // The first two are the built-in modules.
        if index >= BUILTIN_MODULES {
            if specifier.is_empty() || is_relative(specifier) {
                return Err(Failure::Message(format!(
                    "native module `{specifier}` needs a bare specifier"
                )));
            }
            if is_reserved(specifier) {
                return Err(Failure::Message(format!(
                    "native module `{specifier}` is in the `dxn.dtcg` namespace, which is reserved for built-in modules"
                )));
            }
            if options.modules.contains_key(specifier) {
                return Err(Failure::Message(format!(
                    "`{specifier}` is registered both as a JavaScript module and as a native module"
                )));
            }
        }
        table
            .specifiers
            .insert(specifier.to_owned(), specifier.to_owned());
        table.natives.insert(specifier.to_owned());
        table.modules.insert(
            specifier.to_owned(),
            ModuleEntry {
                origin: Origin::Native(specifier.to_owned()),
                language: Language::JavaScript,
                source: native_wrapper(module).into(),
                compiled: None,
                loaded: false,
            },
        );
    }
    for (specifier, module) in &options.modules {
        if specifier.is_empty() || is_relative(specifier) {
            return Err(Failure::Message(format!(
                "host module `{specifier}` needs a bare specifier"
            )));
        }
        if is_reserved(specifier) {
            return Err(Failure::Message(format!(
                "host module `{specifier}` is in the `dxn.dtcg` namespace, which is reserved for built-in modules"
            )));
        }
        table
            .specifiers
            .insert(specifier.clone(), specifier.clone());
        table.modules.insert(
            specifier.clone(),
            ModuleEntry {
                origin: Origin::Host(specifier.clone()),
                language: module.language,
                source: Arc::clone(&module.source),
                compiled: documents
                    .and_then(|documents| documents.compiled.get(specifier).cloned()),
                loaded: false,
            },
        );
    }
    if let Some(documents) = documents {
        for (specifier, resource) in &documents.specifiers {
            table
                .specifiers
                .insert(specifier.clone(), format!("./{resource}"));
        }
        for (resource, (language, source)) in &documents.sources {
            table.modules.insert(
                format!("./{resource}"),
                ModuleEntry {
                    origin: Origin::Document(resource.clone()),
                    language: *language,
                    source: Arc::clone(source),
                    compiled: documents.compiled.get(&format!("./{resource}")).cloned(),
                    loaded: false,
                },
            );
        }
    }
    Ok(table)
}

/// The failure of a module from `origin`, imported as `specifier`.
fn module_failure(origin: &Origin, specifier: &str, message: String) -> Failure {
    match origin {
        Origin::Host(host) | Origin::Native(host) => Failure::Module {
            resource: None,
            specifier: host.clone(),
            message,
        },
        Origin::Document(resource) => Failure::Module {
            resource: Some(resource.clone()),
            specifier: specifier.to_owned(),
            message,
        },
    }
}

/// Why an evaluation failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Failure {
    /// A syntax error, an uncaught exception or a result that is not JSON.
    Message(String),
    /// A module that does not compile or evaluate: a document module
    /// (`resource`) or a host module, and the specifier it was imported by.
    Module {
        resource: Option<ResourceId>,
        specifier: String,
        message: String,
    },
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
    /// A module's own text: positions are authored positions.
    const WHOLE: Self = Self {
        header_lines: 0,
        end: Position {
            line: u32::MAX,
            column: u32::MAX,
        },
    };

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

/// Where a module comes from: the host, or a document of the catalog.
#[derive(Clone, Debug)]
enum Origin {
    Host(String),
    Document(ResourceId),
    /// A native module's generated wrapper (a built-in or the host's).
    Native(String),
}

struct ModuleEntry {
    origin: Origin,
    language: Language,
    source: Arc<str>,
    compiled: Option<Rc<Compiled>>,
    /// Whether QuickJS has declared (and so evaluated) the module.
    loaded: bool,
}

impl ModuleEntry {
    /// The code QuickJS declares for the module `name`, once compiled: a
    /// JavaScript or TypeScript module ends by freezing its values; a
    /// native module's wrapper exports frozen values already.
    fn code(&self, name: &str) -> Option<String> {
        let compiled = self.compiled.as_ref()?;
        Some(match &self.origin {
            Origin::Native(_) => compiled.code.clone(),
            Origin::Host(specifier) => compiled.module_code(specifier),
            Origin::Document(_) => {
                // A relative self-import resolves against the module itself.
                let file = name.rsplit('/').next().unwrap_or(name);
                compiled.module_code(&format!("./{file}"))
            }
        })
    }
}

/// The modules a session can import, by the name QuickJS knows them by: a
/// host module by its specifier, a document module by `./` and its
/// resource id (no bare specifier starts with `./`).
#[derive(Default)]
struct ModuleTable {
    /// Bare specifiers to module names.
    specifiers: HashMap<String, String>,
    modules: HashMap<String, ModuleEntry>,
    /// The native modules: the only importers of [`NATIVE_BRIDGE`].
    natives: HashSet<String>,
}

impl ModuleTable {
    /// The name of the module `name` imports from the module `base`:
    /// relative specifiers resolve against a document module's resource
    /// id, bare ones through the registrations and declarations.
    fn resolve(&self, base: &str, name: &str) -> Result<String, String> {
        if (name == NATIVE_BRIDGE && self.natives.contains(base)) || name == FREEZE_MODULE {
            return Ok(name.to_owned());
        }
        if !is_relative(name) {
            return self
                .specifiers
                .get(name)
                .cloned()
                .ok_or_else(|| format!("unknown module `{name}`"));
        }
        let Some(resource) = base.strip_prefix("./") else {
            return Err(format!(
                "relative import `{name}`: only modules a document declares import relatively"
            ));
        };
        let target = ResourceId::parse(resource)
            .and_then(|resource| resource.resolve(import_reference(name)))
            .map_err(|error| format!("import `{name}`: {error}"))?;
        let target = format!("./{}", target.resource_id());
        if self.modules.contains_key(&target) {
            Ok(target)
        } else {
            Err(format!("unknown module `{name}`"))
        }
    }
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
        self.0
            .borrow()
            .resolve(base, name)
            .map_err(|message| JsError::new_resolving_message(base, name, message))
    }
}

impl loader::Loader for TableLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<loader::ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, rquickjs::module::Declared>> {
        // Every module an expression reaches is compiled before QuickJS
        // loads any of them; the borrow ends before declaring, which loads
        // the module's own imports.
        let code = {
            let mut table = self.0.borrow_mut();
            let entry = table
                .modules
                .get_mut(name)
                .ok_or_else(|| JsError::new_loading_message(name, "unknown module"))?;
            entry.loaded = true;
            entry
                .code(name)
                .ok_or_else(|| JsError::new_loading_message(name, "not compiled"))?
        };
        Module::declare(ctx.clone(), name, code)
    }
}

/// A module on the depth-first walk of an import graph, with its imports
/// (module names and the specifiers that import them).
struct Frame {
    name: String,
    children: Vec<(String, String)>,
    next: usize,
}

/// The state of Tarjan's algorithm over an import graph.
#[derive(Default)]
struct Components {
    index: HashMap<String, usize>,
    low: HashMap<String, usize>,
    stack: Vec<String>,
    on_stack: HashSet<String>,
    /// The specifier each module was first imported by.
    specifiers: HashMap<String, String>,
    /// The components, each after the components it imports, each led by
    /// the member the walk reached first.
    components: Vec<Vec<String>>,
}

pub(crate) struct Session {
    // Persistent handles are released before the context and runtime.
    expressions: HashMap<String, Rc<CompiledExpression>>,
    deep_freeze: Persistent<Function<'static>>,
    drain: Persistent<Function<'static>>,
    overflow: OverflowTest,
    object_prototype: Persistent<Object<'static>>,
    context: Context,
    _runtime: Runtime,
    modules: Rc<RefCell<ModuleTable>>,
    guard: Rc<Guard>,
    precision: Option<u32>,
}

/// The stack limit QuickJS has while a session sets up, at least.
const SETUP_STACK_LIMIT: usize = 256 * 1024;

/// The smallest stack limit QuickJS is given: below it, it cannot even
/// start an evaluation.
const MIN_STACK_LIMIT: usize = 16 * 1024;

impl Session {
    /// Starts QuickJS with the sandbox of `options`, able to import its
    /// host modules and the `documents` modules.
    pub(crate) fn new(
        options: &ExpressionOptions,
        documents: Option<&DocumentModules>,
        extensions: &ValueExtensions,
    ) -> Result<Self, Failure> {
        let natives = builtins::modules(extensions)
            .into_iter()
            .chain(options.native_modules.values().cloned())
            .collect::<Vec<_>>();
        let table = module_table(options, documents, &natives)?;
        let registry = Arc::new(
            natives
                .iter()
                .flat_map(|module| {
                    module
                        .exports()
                        .iter()
                        .filter_map(|export| match &export.export {
                            Export::Function(function) => Some((
                                (module.specifier().to_owned(), export.name.clone()),
                                Arc::clone(function),
                            )),
                            Export::Constant(_) => None,
                        })
                })
                .collect::<HashMap<_, _>>(),
        );
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
        // Setup (hardening, the internal modules) gets a generous stack
        // limit of its own: what it needs varies by platform, and a small
        // `stack_limit` is for expression code only.
        runtime.set_max_stack_size(stack_limit.max(SETUP_STACK_LIMIT));
        runtime.set_interrupt_handler(Some(guard.interrupt_handler()));
        // An async function turns an error into a rejected promise nothing
        // awaits (no jobs run): a stack overflow among them still trips.
        let tracker = Rc::clone(&guard);
        let overflow: OverflowTest = Rc::default();
        let test = Rc::clone(&overflow);
        runtime.set_host_promise_rejection_tracker(Some(Box::new(
            move |ctx, _promise, reason, handled| {
                if !handled && is_stack_overflow(&test, &ctx, &reason) {
                    tracker.trip_stack();
                }
            },
        )));
        let modules = Rc::new(RefCell::new(table));
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
                update_stack_top(&ctx);
                let object_prototype = ctx
                    .globals()
                    .get::<_, Object>("Object")?
                    .get::<_, Object>("prototype")?;
                // Promises exist only because module evaluation needs them:
                // expressions cannot create or await any.
                ctx.globals().remove("Promise")?;
                // The bridge to Rust reaches its module through a global
                // that is gone before any other code runs.
                ctx.globals()
                    .set(BRIDGE_GLOBAL, native_call(&ctx, Arc::clone(&registry))?)?;
                Module::declare(ctx.clone(), NATIVE_BRIDGE, BRIDGE_SOURCE)?
                    .eval()?
                    .1
                    .finish::<()>()?;
                ctx.globals().remove(BRIDGE_GLOBAL)?;
                // The internal module that freezes module values and checks
                // caught values for stack overflows; its check reaches the
                // guard through a global that is gone before any other code
                // runs.
                let trip = Rc::clone(&guard);
                ctx.globals().set(
                    TRIP_GLOBAL,
                    Function::new(ctx.clone(), move || trip.trip_stack())?,
                )?;
                let (freeze_module, evaluated) =
                    Module::declare(ctx.clone(), FREEZE_MODULE, FREEZE_SOURCE)?.eval()?;
                evaluated.finish::<()>()?;
                ctx.globals().remove(TRIP_GLOBAL)?;
                let test: Function = freeze_module.get("isOverflow")?;
                *overflow.borrow_mut() = Some(Persistent::save(&ctx, test));
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
        runtime.set_max_stack_size(stack_limit);
        guard.enforce().map_err(Failure::Limit)?;
        Ok(Self {
            expressions: HashMap::new(),
            deep_freeze,
            overflow,
            drain,
            object_prototype,
            context,
            _runtime: runtime,
            modules,
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
                let compiled = self.compile_expression(&source, expression, span)?;
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
        let result = self.context.with(|ctx| {
            begin(&self.guard, &ctx);
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
                    self.note_overflow(&error);
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

    /// Compiles every module the module `base` imports through `imports`,
    /// transitively, and evaluates those not evaluated yet as ES module
    /// evaluation orders them: every import cycle (a strongly connected
    /// component of the import graph) after the modules it imports, entered
    /// at the member the walk reached first. Each module, or each cycle as a
    /// whole, gets its own step budget.
    fn load_modules(&mut self, base: &str, imports: &[String]) -> Result<(), Failure> {
        let mut roots = Vec::new();
        for specifier in imports {
            let name = self
                .modules
                .borrow()
                .resolve(base, specifier)
                .map_err(Failure::Message)?;
            roots.push((name, specifier.clone()));
        }
        let mut walk = Components::default();
        for (name, specifier) in roots {
            self.connect(&name, &specifier, &mut walk)?;
        }
        for component in &walk.components {
            self.evaluate_component(component, &walk.specifiers)?;
        }
        Ok(())
    }

    /// Whether the module `name` needs no evaluation: QuickJS has it, or it
    /// is no module of the table (the native bridge).
    fn is_evaluated(&self, name: &str) -> bool {
        self.modules
            .borrow()
            .modules
            .get(name)
            .is_none_or(|entry| entry.loaded)
    }

    /// Tarjan's strongly connected components from `root`, without
    /// recursion: completed components join `walk.components` after every
    /// component they import.
    fn connect(&self, root: &str, specifier: &str, walk: &mut Components) -> Result<(), Failure> {
        if self.is_evaluated(root) || walk.index.contains_key(root) {
            return Ok(());
        }
        let mut frames = Vec::new();
        self.visit(root, specifier, walk, &mut frames)?;
        while let Some(frame) = frames.last_mut() {
            if let Some((child, child_specifier)) = frame.children.get(frame.next).cloned() {
                frame.next += 1;
                let parent = frame.name.clone();
                if self.is_evaluated(&child) {
                    continue;
                }
                if !walk.index.contains_key(&child) {
                    self.visit(&child, &child_specifier, walk, &mut frames)?;
                } else if walk.on_stack.contains(&child) {
                    let low = walk.low[&parent].min(walk.index[&child]);
                    walk.low.insert(parent, low);
                }
                continue;
            }
            let frame = frames.pop().expect("a frame");
            if let Some(parent) = frames.last() {
                let low = walk.low[&parent.name].min(walk.low[&frame.name]);
                walk.low.insert(parent.name.clone(), low);
            }
            if walk.low[&frame.name] == walk.index[&frame.name] {
                let mut component = Vec::new();
                loop {
                    let member = walk.stack.pop().expect("a member on the stack");
                    walk.on_stack.remove(&member);
                    let done = member == frame.name;
                    component.push(member);
                    if done {
                        break;
                    }
                }
                // The member reached first leads.
                component.reverse();
                walk.components.push(component);
            }
        }
        Ok(())
    }

    /// Compiles the module `name`, imported as `specifier`, and enters it
    /// into the walk with its imports.
    fn visit(
        &self,
        name: &str,
        specifier: &str,
        walk: &mut Components,
        frames: &mut Vec<Frame>,
    ) -> Result<(), Failure> {
        let compiled = self.compile_module(name, specifier)?;
        let table = self.modules.borrow();
        let mut children = Vec::with_capacity(compiled.imports.len());
        for import in &compiled.imports {
            let child = table.resolve(name, import).map_err(|message| {
                module_failure(&table.modules[name].origin, specifier, message)
            })?;
            children.push((child, import.clone()));
        }
        let index = walk.index.len();
        walk.index.insert(name.to_owned(), index);
        walk.low.insert(name.to_owned(), index);
        walk.stack.push(name.to_owned());
        walk.on_stack.insert(name.to_owned());
        walk.specifiers
            .entry(name.to_owned())
            .or_insert_with(|| specifier.to_owned());
        frames.push(Frame {
            name: name.to_owned(),
            children,
            next: 0,
        });
        Ok(())
    }

    /// The compiled module `name`, compiling it on first use.
    fn compile_module(&self, name: &str, specifier: &str) -> Result<Rc<Compiled>, Failure> {
        let mut table = self.modules.borrow_mut();
        let entry = table.modules.get_mut(name).expect("a resolved module");
        if let Some(compiled) = &entry.compiled {
            return Ok(Rc::clone(compiled));
        }
        let compiled = compile(&entry.source, entry.language).map_err(|error| {
            let message = match error.position {
                Some(position) => format!("{} {}", error.message, at(position)),
                None => error.message,
            };
            module_failure(&entry.origin, specifier, message)
        })?;
        let compiled = Rc::new(compiled);
        entry.compiled = Some(Rc::clone(&compiled));
        Ok(compiled)
    }

    /// Evaluates the top-level code of a module, or of an import cycle from
    /// its leading member, with a full step budget. A failure names the
    /// module of the innermost stack frame: the one that threw.
    fn evaluate_component(
        &self,
        component: &[String],
        specifiers: &HashMap<String, String>,
    ) -> Result<(), Failure> {
        let root = &component[0];
        if self.is_evaluated(root) {
            return Ok(());
        }
        self.drain_counter();
        let result = self.context.with(|ctx| {
            begin(&self.guard, &ctx);
            let code = {
                let mut table = self.modules.borrow_mut();
                let entry = table.modules.get_mut(root.as_str()).expect("a module");
                entry.loaded = true;
                entry.code(root).expect("compiled")
            };
            let evaluated = Module::declare(ctx.clone(), root.as_str(), code)
                .and_then(Module::eval)
                .and_then(|(_, promise)| match promise.result::<JsValue>() {
                    Some(result) => result.map(|_| None),
                    None => Ok(Some(())),
                })
                .catch(&ctx);
            match evaluated {
                Ok(None) => Ok(()),
                Ok(Some(())) => Err(self.module_error(
                    root,
                    specifiers,
                    "top-level `await` is not available in expression modules".into(),
                )),
                Err(error) => {
                    self.note_overflow(&error);
                    let stack = match &error {
                        CaughtError::Exception(exception) => exception.stack().unwrap_or_default(),
                        _ => String::new(),
                    };
                    let thrower = self.thrower(&stack).unwrap_or_else(|| root.clone());
                    let compiled = self.modules.borrow().modules[&thrower].compiled.clone();
                    let locator = compiled.as_deref().map(|compiled| Locator {
                        name: &thrower,
                        compiled,
                        span: Span::WHOLE,
                    });
                    Err(self.module_error(&thrower, specifiers, caught_message(&error, locator)))
                }
            }
        });
        match self.guard.end() {
            Some(trip) => Err(Failure::Limit(trip)),
            None => result,
        }
    }

    /// Records a stack overflow that reached the engine uncaught, whether
    /// or not an allocation past the stack threshold noticed it.
    fn note_overflow(&self, error: &CaughtError<'_>) {
        if let CaughtError::Exception(exception) = error
            && is_stack_overflow(&self.overflow, exception.ctx(), exception.as_value())
        {
            self.guard.trip_stack();
        }
    }

    /// The module of the innermost stack frame that names one: frames read
    /// `    at function (module:line:column)` or `    at module:line:column`.
    fn thrower(&self, stack: &str) -> Option<String> {
        let table = self.modules.borrow();
        stack.lines().find_map(|line| {
            let location = line
                .trim()
                .strip_prefix("at ")?
                .rsplit_once(" (")
                .map_or(line.trim().strip_prefix("at ")?, |(_, location)| location)
                .trim_end_matches(')');
            let mut parts = location.rsplitn(3, ':');
            let (_, _, name) = (parts.next()?, parts.next()?, parts.next()?);
            table.modules.contains_key(name).then(|| name.to_owned())
        })
    }

    /// The failure of the module `name`, with the specifier that imported it.
    fn module_error(
        &self,
        name: &str,
        specifiers: &HashMap<String, String>,
        message: String,
    ) -> Failure {
        let table = self.modules.borrow();
        let specifier = specifiers.get(name).map_or(name, String::as_str);
        module_failure(&table.modules[name].origin, specifier, message)
    }

    /// Compiles and evaluates the generated module of an expression and
    /// keeps its default export.
    fn compile_expression(
        &mut self,
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
        // Tokens with the same expression text share its module, so its name
        // comes from the text, not from whichever token compiled it first.
        let digest = Sha256::digest(source.as_bytes());
        let name = format!(
            "dxn.dtcg/expression/{}",
            digest[..8]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        self.load_modules(&name, &compiled.imports)?;
        let function = self.context.with(|ctx| {
            begin(&self.guard, &ctx);
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
                .map_err(|error| {
                    self.note_overflow(&error);
                    Failure::Message(caught_message(&error, Some(locator)))
                })?;
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
        let line = line.parse().ok()?;
        let generated = Position {
            line,
            column: locator.compiled.char_column(line, column.parse().ok()?),
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

/// The internal module's `isOverflow`, once the session has set it up.
type OverflowTest = Rc<RefCell<Option<Persistent<Function<'static>>>>>;

/// Whether `value` is a stack overflow QuickJS reported, also one wrapped
/// in `SuppressedError`s or `AggregateError`s, as the internal module's
/// `isOverflow` decides (its walk runs under the step budget).
fn is_stack_overflow<'js>(test: &OverflowTest, ctx: &Ctx<'js>, value: &JsValue<'js>) -> bool {
    let Some(function) = test.borrow().clone() else {
        return false;
    };
    function
        .restore(ctx)
        .and_then(|function| function.call::<_, bool>((value.clone(),)))
        .unwrap_or(false)
}

/// Starts an evaluation in the current frame: the guard's stack base and
/// QuickJS's stack top mark the same place, so QuickJS's own stack limit
/// and the guard's threshold measure the same depth. rquickjs moves the
/// stack top only with its `parallel` feature, so it would otherwise stay
/// where the session was created.
fn begin(guard: &Guard, ctx: &Ctx<'_>) {
    guard.begin();
    update_stack_top(ctx);
}

/// Makes the current frame QuickJS's stack top.
fn update_stack_top(ctx: &Ctx<'_>) {
    // SAFETY: `ctx` is a live context of this session's runtime, used on
    // the thread that owns it inside `Context::with`.
    unsafe {
        rquickjs::qjs::JS_UpdateStackTop(rquickjs::qjs::JS_GetRuntime(ctx.as_raw().as_ptr()));
    }
}

/// How many of a session's native modules are built in.
const BUILTIN_MODULES: usize = 2;

/// The internal module only native module wrappers import.
const NATIVE_BRIDGE: &str = "dxn.dtcg/native";

/// The internal module that freezes a module's values after its top-level
/// code: every value reachable from its arguments, functions, accessors
/// and prototypes included. A module namespace refuses freezing (its
/// bindings stay live), and a binding still uninitialized in an import
/// cycle is skipped: its own module freezes it.
const FREEZE_SOURCE: &str = r#"
const trip = globalThis.__dxn_dtcg_trip;
const { freeze, getOwnPropertyDescriptor, getPrototypeOf } = Object;
const { ownKeys } = Reflect;
const { get } = Reflect;
const rangeError = RangeError.prototype;
const syntaxError = SyntaxError.prototype;
const suppressedError = typeof SuppressedError === "function" ? SuppressedError.prototype : null;
const aggregateError = AggregateError.prototype;
const isArray = Array.isArray;
const own = (object, key) => {
  const descriptor = getOwnPropertyDescriptor(object, key);
  return descriptor !== undefined && "value" in descriptor ? descriptor.value : undefined;
};
// Whether a caught value is a stack overflow (of calls, or of the regular
// expression compiler), also one wrapped in `SuppressedError`s or
// `AggregateError`s however deeply: every wrapped value is visited once,
// read through data properties only, so no getter of the code runs.
const isOverflow = (caught) => {
  const pending = [caught];
  const seen = new WeakSet();
  while (pending.length > 0) {
    const value = pending.pop();
    if (typeof value !== "object" || value === null || seen.has(value)) continue;
    seen.add(value);
    const prototype = getPrototypeOf(value);
    if (prototype === rangeError || prototype === syntaxError) {
      const message = own(value, "message");
      if ((prototype === rangeError && message === "Maximum call stack size exceeded")
          || (prototype === syntaxError && message === "stack overflow")) {
        return true;
      }
    } else if (prototype === suppressedError) {
      pending.push(own(value, "error"), own(value, "suppressed"));
    } else if (prototype === aggregateError) {
      const errors = own(value, "errors");
      if (isArray(errors)) for (let index = 0; index < errors.length; index++) pending.push(own(errors, index));
    }
  }
  return false;
};
// Every `catch` and `finally` of compiled code passes what it caught here
// first: a stack overflow stops the evaluation instead of being handled.
const check = (caught) => {
  if (isOverflow(caught)) trip();
};
const { defineProperty } = Object;
const TypeErrorConstructor = TypeError;
const objectPrototype = Object.prototype;
const arrayPrototype = Array.prototype;
const functionPrototype = Function.prototype;
const done = new WeakSet();
const isObject = (value) => (typeof value === "object" && value !== null) || typeof value === "function";
// A frozen object that other objects inherit from would make assigning one
// of its properties on an inheriting object fail. Its data properties become
// accessors whose setter gives the assigned object its own property, as the
// built-ins' are.
const tame = (object) => {
  for (const key of ownKeys(object)) {
    const descriptor = getOwnPropertyDescriptor(object, key);
    if (!("value" in descriptor) || !descriptor.configurable) continue;
    const value = descriptor.value;
    const accessors = {
      get() { return value; },
      set(next) {
        if (this === object) throw new TypeErrorConstructor("'" + String(key) + "' is read-only");
        defineProperty(this, key, { value: next, writable: true, enumerable: true, configurable: true });
      },
    };
    freeze(accessors.get);
    freeze(accessors.set);
    defineProperty(object, key, { get: accessors.get, set: accessors.set, enumerable: descriptor.enumerable, configurable: false });
  }
};
const freezeAll = (...roots) => {
  // First every value reachable (the list is also the work queue), and
  // which of them others inherit from; then taming and freezing, so taming
  // runs before an object freezes.
  const found = [];
  const prototypes = new Set();
  const add = (value) => {
    if (!done.has(value)) { done.add(value); found.push(value); }
  };
  for (const root of roots) {
    if (isObject(root)) add(root);
  }
  for (let index = 0; index < found.length; index++) {
    const value = found[index];
    // A proxy may throw, and a module namespace throws for a binding still
    // uninitialized in an import cycle: what that one key holds is skipped.
    let keys;
    try { keys = ownKeys(value); } catch (caught) { check(caught); continue; }
    for (const key of keys) {
      let descriptor;
      try { descriptor = getOwnPropertyDescriptor(value, key); } catch (caught) { check(caught); continue; }
      if (descriptor === undefined) continue;
      if ("value" in descriptor) {
        const child = descriptor.value;
        if (isObject(child)) {
          if (key === "prototype" && typeof value === "function") prototypes.add(child);
          add(child);
        }
      } else {
        if (descriptor.get !== undefined) add(descriptor.get);
        if (descriptor.set !== undefined) add(descriptor.set);
      }
    }
    let prototype;
    try { prototype = getPrototypeOf(value); } catch (caught) { check(caught); continue; }
    if (prototype !== null && prototype !== objectPrototype && prototype !== arrayPrototype
        && prototype !== functionPrototype) {
      prototypes.add(prototype);
      add(prototype);
    }
  }
  for (const value of found) {
    try {
      if (prototypes.has(value)) tame(value);
      freeze(value);
    } catch (caught) {
      check(caught);
    }
  }
};
// Runs the body of an iterator's `return` method, checking what it throws
// before a closing loop or built-in discards it.
const checked = (body, ...parameters) => {
  try { return body(...parameters); } catch (caught) { check(caught); throw caught; }
};
freeze(isOverflow);
freeze(check);
freeze(checked);
freeze(freezeAll);
export { check, checked, freezeAll, isOverflow };
"#;

/// The global that hands the guard's stack trip to [`FREEZE_SOURCE`]
/// during setup.
const TRIP_GLOBAL: &str = "__dxn_dtcg_trip";

/// The global that hands the bridge function to [`BRIDGE_SOURCE`] during
/// setup.
const BRIDGE_GLOBAL: &str = "__dxn_dtcg_native_call";

/// The bridge module: the Rust call function, JSON text in and out, and a
/// deep freeze for constants.
const BRIDGE_SOURCE: &str = r#"
const native = globalThis.__dxn_dtcg_native_call;
const { parse, stringify } = JSON;
const freeze = (value) => {
  if (((typeof value === "object" && value !== null) || typeof value === "function") && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const item of Object.values(value)) freeze(item);
  }
  return value;
};
const { isFinite } = Number;
const TypeErrorConstructor = TypeError;
// JSON has no NaN or Infinity: they would arrive as `null`.
const call = (specifier, name, args) => parse(native(specifier, name, stringify(args, function (key, value) {
  if (typeof value === "number" && !isFinite(value)) {
    throw new TypeErrorConstructor(this === args
      ? specifier + "." + name + ": argument " + (Number(key) + 1) + " must be a finite number"
      : specifier + "." + name + ": arguments must not contain NaN or Infinity");
  }
  return value;
})));
Object.freeze(native);
export { call, freeze };
"#;

/// The ES module of a native module: each function export calls into Rust
/// through the bridge, each constant is its frozen JSON value.
fn native_wrapper(module: &NativeModule) -> String {
    let specifier = Value::String(module.specifier().to_owned());
    let mut source = format!("import {{ call, freeze }} from {:?};\n", NATIVE_BRIDGE);
    for (index, export) in module.exports().iter().enumerate() {
        let name = Value::String(export.name.clone());
        match &export.export {
            Export::Function(_) => {
                let _ = writeln!(
                    source,
                    "const __export{index} = freeze((...args) => call({specifier}, {name}, args));"
                );
            }
            Export::Constant(value) => {
                let text = Value::String(value.to_string());
                let _ = writeln!(
                    source,
                    "const __export{index} = freeze(JSON.parse({text}));"
                );
            }
        }
        let _ = writeln!(source, "export {{ __export{index} as {name} }};");
    }
    source
}

/// The Rust side of the bridge: calls the native function `name` of the
/// module `specifier` with JSON arguments. An `Err` or a panic throws an
/// `Error`, and arguments that do not convert a `TypeError`, each prefixed
/// with `<specifier>.<name>:`.
fn native_call<'js>(
    ctx: &Ctx<'js>,
    registry: Arc<HashMap<(String, String), NativeFn>>,
) -> rquickjs::Result<Function<'js>> {
    Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, specifier: String, name: String, arguments: String| {
            let qualified = format!("{specifier}.{name}");
            let Some(function) = registry.get(&(specifier, name)) else {
                return Err(rquickjs::Exception::throw_type(
                    &ctx,
                    &format!("{qualified}: no such native function"),
                ));
            };
            let arguments: Vec<Value> = serde_json::from_str(&arguments).map_err(|error| {
                rquickjs::Exception::throw_type(&ctx, &format!("{qualified}: {error}"))
            })?;
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| function(&arguments)));
            match result {
                Ok(Ok(value)) => Ok(value.to_string()),
                Ok(Err(NativeError::Type(message))) => Err(rquickjs::Exception::throw_type(
                    &ctx,
                    &format!("{qualified}: {message}"),
                )),
                Ok(Err(NativeError::Error(message))) => Err(rquickjs::Exception::throw_message(
                    &ctx,
                    &format!("{qualified}: {message}"),
                )),
                Err(panic) => {
                    let message = panic
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_owned())
                        .or_else(|| panic.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "the function panicked".into());
                    Err(rquickjs::Exception::throw_message(
                        &ctx,
                        &format!("{qualified}: panicked: {message}"),
                    ))
                }
            }
        },
    )
}
