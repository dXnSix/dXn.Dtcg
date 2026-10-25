//! JavaScript and TypeScript expressions as computed token values (feature
//! `expressions`).
//!
//! A token computes its value with an expression in
//! `$extensions["dxn.dtcg.expressions"]` ([`EXPRESSION_EXTENSION`]) while
//! its `$value` stays the DTCG-valid snapshot. A runtime evaluates
//! expressions only when its options enable them:
//!
//! ```
//! use dxn_dtcg::{DtcgRuntime, RuntimeOptions, expressions::ExpressionOptions};
//!
//! let options = RuntimeOptions::default().with_expressions(ExpressionOptions::default());
//! let runtime = DtcgRuntime::from_json_with_options(
//!     "tokens.json",
//!     r#"{
//!       "base": { "$type": "number", "$value": 4 },
//!       "large": {
//!         "$type": "number",
//!         "$value": 16,
//!         "$extensions": {
//!           "dxn.dtcg.expressions": { "inputs": { "base": "{base}" }, "js": "base * 4" }
//!         }
//!       }
//!     }"#,
//!     options,
//! )?;
//! let tokens = runtime.resolve(&[])?.ready().expect("no missing documents");
//! assert_eq!(tokens.get("large").unwrap().value, 16);
//! # Ok::<(), dxn_dtcg::DtcgError>(())
//! ```
//!
//! [`EXPRESSION_EXTENSION`]: crate::EXPRESSION_EXTENSION

mod compile;
mod engine;
mod format;
mod modules;
mod nesting;
mod sandbox;

pub use modules::ExpressionModule;
pub(crate) use modules::{DocumentModules, document_modules, validate_modules};

pub(crate) use engine::{Failure, Session};

pub(crate) use format::token_expression;
pub(crate) use sandbox::Trip;

/// QuickJS's clock on `wasm32-unknown-unknown`, which its shim otherwise
/// imports from the host as `env.__rquickjs_host_now_us`. Expressions have
/// no clock (`Date` is not installed), so it is always 0, and the module
/// needs no QuickJS-specific host import.
///
/// The symbol belongs to the whole WebAssembly module, which links one copy
/// of QuickJS: other code in the same module that uses `quickjs-jit`
/// directly reads this clock too (`Date.now()` is 0), and cannot supply
/// its own `env.__rquickjs_host_now_us`.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[unsafe(no_mangle)]
extern "C" fn __rquickjs_host_now_us() -> f64 {
    0.0
}

/// How expressions evaluate: the sandbox's limits and the rounding of
/// results. Construct it with [`ExpressionOptions::default`] and the
/// chainable `with_*` setters, and enable it with
/// [`RuntimeOptions::with_expressions`](crate::RuntimeOptions::with_expressions).
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ExpressionOptions {
    /// The work one evaluation (an expression call or a module's top-level
    /// code) may do, in QuickJS interrupt checks, which QuickJS makes about
    /// every 10,000 function calls and loop iterations (default 1,000).
    /// Counting checks instead of time keeps the budget deterministic; a
    /// budget below 1 counts as 1. Exceeding it fails with
    /// `LimitExceeded { kind: "expression steps" }`.
    pub step_budget: usize,
    /// Bytes the JavaScript engine of one resolution may allocate (default
    /// 64 MiB). Exceeding it fails with
    /// `LimitExceeded { kind: "expression memory" }`.
    pub memory_limit: usize,
    /// Bytes of native stack JavaScript code may use below the resolving
    /// call (default 256 KiB, which fits the default 1 MiB stack of
    /// `wasm32-unknown-unknown`; at least 16 KiB, so smaller values count as
    /// 16 KiB). Runaway recursion fails with `DtcgError::Expression` (stack
    /// overflow). The limit must leave room on the stack of the thread that
    /// resolves: a limit near that stack's size lets QuickJS overflow it
    /// before its own check, which aborts the process.
    pub stack_limit: usize,
    /// Decimal places every non-integral number of a result is rounded to
    /// (default 6); `None` keeps results unrounded. Rounding keeps results
    /// equal across platforms whose math libraries differ in the last bit.
    pub precision: Option<u32>,
    /// Host modules by bare specifier.
    pub(crate) modules: std::collections::BTreeMap<String, ExpressionModule>,
}

impl Default for ExpressionOptions {
    fn default() -> Self {
        Self {
            step_budget: 1_000,
            memory_limit: 64 * 1024 * 1024,
            stack_limit: 256 * 1024,
            precision: Some(6),
            modules: std::collections::BTreeMap::new(),
        }
    }
}

impl ExpressionOptions {
    /// Sets [`ExpressionOptions::step_budget`].
    pub fn with_step_budget(mut self, budget: usize) -> Self {
        self.step_budget = budget;
        self
    }

    /// Sets [`ExpressionOptions::memory_limit`].
    pub fn with_memory_limit(mut self, limit: usize) -> Self {
        self.memory_limit = limit;
        self
    }

    /// Sets [`ExpressionOptions::stack_limit`].
    pub fn with_stack_limit(mut self, limit: usize) -> Self {
        self.stack_limit = limit;
        self
    }

    /// Sets [`ExpressionOptions::precision`].
    pub fn with_precision(mut self, precision: Option<u32>) -> Self {
        self.precision = precision;
        self
    }

    /// Registers `module` under the bare `specifier` (replacing an earlier
    /// registration), for expressions to import through their `imports`
    /// and for modules to import. A specifier in the reserved `dxn.dtcg`
    /// namespace, or one a token document also declares, fails the
    /// resolutions that evaluate expressions.
    pub fn with_module(mut self, specifier: impl Into<String>, module: ExpressionModule) -> Self {
        self.modules.insert(specifier.into(), module);
        self
    }

    /// The host modules by specifier.
    pub fn modules(&self) -> impl Iterator<Item = (&str, &ExpressionModule)> {
        self.modules
            .iter()
            .map(|(specifier, module)| (specifier.as_str(), module))
    }
}
