//! Native modules: Rust functions and constants that expressions and
//! modules import like any ES module.

use std::{fmt, sync::Arc};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

/// Why a native function call failed: a JavaScript `TypeError` for
/// arguments that do not convert, an `Error` for everything else.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NativeError {
    Type(String),
    Error(String),
}

pub(crate) type NativeFn = Arc<dyn Fn(&[Value]) -> Result<Value, NativeError> + Send + Sync>;

/// One export of a native module.
#[derive(Clone)]
pub(crate) enum Export {
    Function(NativeFn),
    Constant(Value),
}

/// An export with what declarations say about it.
#[derive(Clone)]
pub(crate) struct NativeExport {
    pub(crate) name: String,
    pub(crate) export: Export,
    /// The TypeScript type of the export, such as `(value: number) => number`.
    pub(crate) typescript: Option<String>,
    /// Documentation, written as TSDoc in declarations.
    pub(crate) docs: Option<String>,
}

#[derive(Clone)]
struct Inner {
    specifier: String,
    docs: Option<String>,
    exports: Vec<NativeExport>,
}

/// A module of Rust functions and constants that expressions import by its
/// specifier, like a JavaScript module.
///
/// Functions take and return serde types; arguments arrive as JSON values
/// (a missing argument is `null`), and a function may fail by returning
/// `Err` with any [`Display`](fmt::Display) error. An `Err`, or a panic,
/// becomes a JavaScript `Error` whose message starts with
/// `<specifier>.<name>:`; an argument that does not deserialize becomes a
/// `TypeError`. Functions must be pure: deterministic, without I/O, reading
/// nothing but their arguments.
///
/// ```
/// use dxn_dtcg::expressions::NativeModule;
///
/// let module = NativeModule::new("brand/math")
///     .function("double", |value: f64| value * 2.0)
///     .function("ratio", |a: f64, b: f64| {
///         if b == 0.0 { Err("division by zero") } else { Ok(a / b) }
///     })
///     .constant("golden", 1.618);
/// assert_eq!(module.specifier(), "brand/math");
/// ```
///
/// Clones are the same module: equality compares identity.
#[derive(Clone)]
pub struct NativeModule {
    inner: Arc<Inner>,
}

impl NativeModule {
    /// An empty module imported as `specifier`, a bare module specifier
    /// outside the reserved `dxn.dtcg` namespace.
    pub fn new(specifier: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(Inner {
                specifier: specifier.into(),
                docs: None,
                exports: Vec::new(),
            }),
        }
    }

    /// The specifier expressions import this module by.
    pub fn specifier(&self) -> &str {
        &self.inner.specifier
    }

    /// Exports `function` as `name`. It takes up to eight arguments of
    /// types that implement `DeserializeOwned` and returns a
    /// [`NativeReturn`]: a JSON-like value (numbers, strings, booleans, `()`,
    /// `serde_json::Value`, vectors, options and maps of them, the
    /// [`values`](crate::values) models), any serializable value wrapped in
    /// [`Json`], or `Result<T, E>` with a serializable `T` and an `E` that
    /// implements `Display`.
    ///
    /// # Panics
    ///
    /// When `name` is not a JavaScript identifier or is a reserved word
    /// (such as `new` or `default`).
    pub fn function<Args, F>(self, name: impl Into<String>, function: F) -> Self
    where
        F: NativeFunction<Args>,
    {
        let name = name.into();
        let wrapped = function.into_native().0;
        self.export(name, Export::Function(wrapped))
    }

    /// Exports the JSON form of `value` as the constant `name`.
    ///
    /// # Panics
    ///
    /// When `value` does not serialize to JSON, or `name` is not a
    /// JavaScript identifier or is a reserved word.
    pub fn constant(self, name: impl Into<String>, value: impl Serialize) -> Self {
        let value = serde_json::to_value(value).expect("a native module constant is JSON");
        self.export(name.into(), Export::Constant(value))
    }

    /// Describes the export `name` for
    /// [`ExpressionOptions::typescript_declarations`](super::ExpressionOptions::typescript_declarations):
    /// its TypeScript type (such as `(value: number) => number`) and its
    /// documentation.
    pub fn describe(
        mut self,
        name: &str,
        typescript: impl Into<String>,
        docs: impl Into<String>,
    ) -> Self {
        let inner = Arc::make_mut(&mut self.inner);
        if let Some(export) = inner.exports.iter_mut().find(|export| export.name == name) {
            export.typescript = Some(typescript.into());
            let docs = docs.into();
            export.docs = (!docs.is_empty()).then_some(docs);
        }
        self
    }

    /// Documents the module itself in declarations.
    pub fn with_docs(mut self, docs: impl Into<String>) -> Self {
        let docs = docs.into();
        Arc::make_mut(&mut self.inner).docs = (!docs.is_empty()).then_some(docs);
        self
    }

    fn export(self, name: String, export: Export) -> Self {
        self.raw_export(NativeExport {
            name,
            export,
            typescript: None,
            docs: None,
        })
    }

    /// Adds an export as is (the built-in modules' functions throw
    /// `TypeError`s of their own).
    pub(crate) fn raw_export(mut self, export: NativeExport) -> Self {
        if let Err(problem) = super::format::check_identifier(&export.name) {
            panic!(
                "native module `{}`: export name `{}` {problem}",
                self.inner.specifier, export.name
            );
        }
        let inner = Arc::make_mut(&mut self.inner);
        inner
            .exports
            .retain(|existing| existing.name != export.name);
        inner.exports.push(export);
        self
    }

    pub(crate) fn exports(&self) -> &[NativeExport] {
        &self.inner.exports
    }

    pub(crate) fn docs(&self) -> Option<&str> {
        self.inner.docs.as_deref()
    }
}

/// Prints the specifier and the export names, never closures.
impl fmt::Debug for NativeModule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeModule")
            .field("specifier", &self.inner.specifier)
            .field(
                "exports",
                &self
                    .inner
                    .exports
                    .iter()
                    .map(|export| export.name.as_str())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl PartialEq for NativeModule {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for NativeModule {}

/// What a native function returns: a serializable value, or a `Result`
/// whose error is reported by its `Display` text.
pub trait NativeReturn {
    #[doc(hidden)]
    fn into_result(self) -> Result<Value, String>;
}

/// Any serializable value as a native function's result, for types the
/// plain [`NativeReturn`] implementations do not cover:
/// `|pair: Pair| Json(Pair { a: pair.b, b: pair.a })`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Json<T>(pub T);

impl<T: Serialize> NativeReturn for Json<T> {
    fn into_result(self) -> Result<Value, String> {
        serde_json::to_value(self.0).map_err(|error| error.to_string())
    }
}

impl<T: Serialize, E: fmt::Display> NativeReturn for Result<T, E> {
    fn into_result(self) -> Result<Value, String> {
        match self {
            Ok(value) => serde_json::to_value(value).map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        }
    }
}

macro_rules! plain_returns {
    ($($type:ty),* $(,)?) => {
        $(
            impl NativeReturn for $type {
                fn into_result(self) -> Result<Value, String> {
                    serde_json::to_value(self).map_err(|error| error.to_string())
                }
            }
        )*
    };
}

plain_returns!(
    (),
    bool,
    char,
    f32,
    f64,
    i8,
    i16,
    i32,
    i64,
    isize,
    u8,
    u16,
    u32,
    u64,
    usize,
    String,
    &'static str,
    Value,
    crate::values::Color,
    crate::values::Dimension,
    crate::values::Duration,
    crate::values::CubicBezier,
    crate::values::FontFamily,
    crate::values::FontWeight,
    crate::values::StrokeStyle,
    crate::values::Border,
    crate::values::Transition,
    crate::values::Shadow,
    crate::values::Gradient,
    crate::values::Typography,
);

impl<T: Serialize> NativeReturn for Vec<T> {
    fn into_result(self) -> Result<Value, String> {
        serde_json::to_value(self).map_err(|error| error.to_string())
    }
}

impl<T: Serialize> NativeReturn for Option<T> {
    fn into_result(self) -> Result<Value, String> {
        serde_json::to_value(self).map_err(|error| error.to_string())
    }
}

impl<T: Serialize, const N: usize> NativeReturn for [T; N] {
    fn into_result(self) -> Result<Value, String> {
        serde_json::to_value(self.as_slice()).map_err(|error| error.to_string())
    }
}

impl<T: Serialize> NativeReturn for std::collections::BTreeMap<String, T> {
    fn into_result(self) -> Result<Value, String> {
        serde_json::to_value(self).map_err(|error| error.to_string())
    }
}

/// A closure a [`NativeModule`] can export: `Fn(A1, …, An) -> R` with up to
/// eight deserializable arguments and a [`NativeReturn`].
pub trait NativeFunction<Args>: Send + Sync + 'static {
    #[doc(hidden)]
    fn into_native(self) -> ErasedFunction;
}

/// A native function with its argument and result types erased.
#[doc(hidden)]
pub struct ErasedFunction(pub(crate) NativeFn);

fn argument<T: DeserializeOwned>(arguments: &[Value], index: usize) -> Result<T, NativeError> {
    let value = arguments.get(index).cloned().unwrap_or(Value::Null);
    serde_json::from_value(value)
        .map_err(|error| NativeError::Type(format!("argument {}: {error}", index + 1)))
}

macro_rules! native_functions {
    ($(($($arg:ident $index:tt),*)),* $(,)?) => {
        $(
            impl<F, R, $($arg),*> NativeFunction<($($arg,)*)> for F
            where
                F: Fn($($arg),*) -> R + Send + Sync + 'static,
                R: NativeReturn,
                $($arg: DeserializeOwned,)*
            {
                #[allow(unused_variables)]
                fn into_native(self) -> ErasedFunction {
                    ErasedFunction(Arc::new(move |arguments: &[Value]| {
                        let result = self($(argument::<$arg>(arguments, $index)?),*);
                        result.into_result().map_err(NativeError::Error)
                    }))
                }
            }
        )*
    };
}

native_functions!(
    (),
    (A0 0),
    (A0 0, A1 1),
    (A0 0, A1 1, A2 2),
    (A0 0, A1 1, A2 2, A3 3),
    (A0 0, A1 1, A2 2, A3 3, A4 4),
    (A0 0, A1 1, A2 2, A3 3, A4 4, A5 5),
    (A0 0, A1 1, A2 2, A3 3, A4 4, A5 5, A6 6),
    (A0 0, A1 1, A2 2, A3 3, A4 4, A5 5, A6 6, A7 7),
);
