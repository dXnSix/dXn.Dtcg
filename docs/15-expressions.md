# 15. Expressions

Expressions compute a token's value with JavaScript or TypeScript. A spacing scale, a hover color
or a type ramp can be derived from other tokens instead of copied by hand, and the derivation lives
in the token document next to the value it produces. This chapter describes the format, how
resolution evaluates it, the modules expressions import, the Rust functions they call, the sandbox
they run in, and how to use them on WebAssembly.

- [Why an extension](#why-an-extension)
- [Enabling expressions](#enabling-expressions)
- [The expression object](#the-expression-object)
- [How resolution evaluates an expression](#how-resolution-evaluates-an-expression)
- [Results](#results)
- [TypeScript](#typescript)
- [Errors](#errors)
- [Modules](#modules)
- [Modules in token documents](#modules-in-token-documents)
- [Rust functions: native modules](#rust-functions-native-modules)
- [The `#[expression_module]` macro](#the-expression_module-macro)
- [Built-in modules](#built-in-modules)
- [The sandbox and its limits](#the-sandbox-and-its-limits)
- [TypeScript declarations for editors](#typescript-declarations-for-editors)
- [WebAssembly](#webassembly)
- [Determinism](#determinism)
- [What expressions do not do](#what-expressions-do-not-do)

## Why an extension

DTCG 2025.10 has no computed values. Format §8 requires a token's `$value` to match its `$type`:
a dimension is `{ "value": 16, "unit": "px" }`, never a formula. Format §5.2.3 lets tools put
their own data under `$extensions` and requires other tools to preserve it, while recommending that
it stay optional metadata. The community group closed computed values as out of scope for the
format, so an extension is the route the specification leaves open.

An expression therefore lives in the token's `$extensions`, under the key
`dxn.dtcg.expressions` (`dxn_dtcg::EXPRESSION_EXTENSION`), and the token keeps a valid `$value`:

```json
{
  "space": {
    "$type": "dimension",
    "base": { "$value": { "value": 4, "unit": "px" } },
    "large": {
      "$value": { "value": 16, "unit": "px" },
      "$extensions": {
        "dxn.dtcg.expressions": {
          "inputs": { "base": "{space.base}" },
          "js": "({ ...base, value: base.value * 4 })"
        }
      }
    }
  }
}
```

The `$value` is the *snapshot*: what every DTCG tool that ignores the extension reads, including
this runtime with expressions off. With expressions on, `dxn_dtcg` evaluates the expression and
uses its result instead. A document with expressions is a valid DTCG 2025.10 document either way.

## Enabling expressions

Expressions are off twice. The Cargo feature `expressions` compiles the JavaScript engine
(QuickJS, through the `quickjs-jit` crate) and the TypeScript compiler (oxc) in:

```toml
[dependencies]
dxn_dtcg = { git = "https://github.com/dXnSix/dXn.Dtcg", tag = "vX.Y.Z", features = ["expressions"] }
```

Then `RuntimeOptions::with_expressions` turns evaluation on for one runtime:

```rust
use dxn_dtcg::{DtcgRuntime, RuntimeOptions, expressions::ExpressionOptions};

let options = RuntimeOptions::default().with_expressions(ExpressionOptions::default());
let runtime = DtcgRuntime::from_json_with_options(
    "tokens.json",
    r#"{
      "space": {
        "$type": "dimension",
        "base": { "$value": { "value": 4, "unit": "px" } },
        "large": {
          "$value": { "value": 16, "unit": "px" },
          "$extensions": { "dxn.dtcg.expressions": {
            "inputs": { "base": "{space.base}" },
            "js": "({ ...base, value: base.value * 5 })"
          } }
        }
      }
    }"#,
    options,
)?;
let tokens = runtime.resolve(&[])?.ready().unwrap();
let large = tokens.get("space.large").unwrap();
assert_eq!(large.value, serde_json::json!({ "value": 20, "unit": "px" }));
// The authored `$value`, resolved, is kept as the snapshot.
assert_eq!(large.snapshot, Some(serde_json::json!({ "value": 16, "unit": "px" })));
# Ok::<(), dxn_dtcg::DtcgError>(())
```

Without the option, the same runtime resolves `space.large` to its snapshot, keeps the extension
object unchanged in `ResolvedToken::extensions`, and never validates it. Because the option, not
the feature, decides, enabling the feature in one crate of a build (Cargo unifies features) never
changes how another crate's runtime resolves.

The default build compiles no C code and stays free of the engine; the feature adds QuickJS's C
sources (built with the platform C compiler) and oxc.

## The expression object

The object at `$extensions["dxn.dtcg.expressions"]` of a token has these members:

| Member | Required | Value |
|---|---|---|
| `js` or `ts` | exactly one | The expression: one JavaScript or TypeScript expression (not statements), as a string |
| `inputs` | no | An object from input names to DTCG values: literals, `{curly}` aliases or JSON Pointer `$ref` objects, also nested inside objects and arrays |
| `imports` | no | An object from module specifiers to an array of imported export names (named imports) or to one name (a namespace import) |

Rules, checked only when expressions are enabled:

- The object has no other members; `js` and `ts` are strings.
- Input names and imported names are JavaScript identifiers (ECMAScript identifier syntax, without
  escapes) that are not reserved words in an ES module, including `await`, `eval` and `arguments`.
  Names are unique across `inputs` and `imports`.
- Import specifiers are bare (`"brand/math"`), not relative (`"./math.js"`) and not empty.
- `js` and `ts` hold exactly one expression: text that would close the generated function early
  (`"1), (2"`) is a syntax error. Use an arrow function to run statements:
  `"(() => { const x = base.value * 2; return x + 1; })()"`.
- The token still needs `$value` (or a token-level `$ref`), as every DTCG token does.

A violation fails with `DtcgError::Expression { resource, path, message }`, naming the token and
the member. With expressions off, the object is opaque: even a malformed one is kept and ignored.

## How resolution evaluates an expression

A token with an expression resolves in two steps:

1. Its `$value` resolves exactly as without expressions: references, the effective type (own
   `$type`, then the type a whole-value reference targets, then the group's) and validation. An
   invalid snapshot is still an error, and the token has the same type with expressions on or off.
2. Its `inputs` resolve like token values at a position without type rules: aliases, JSON
   Pointers (also into document locations such as `#/$extensions/…`) and literals. Then the
   expression runs with each input bound, by name, to a frozen JSON value. Its result replaces
   the value and is validated against the token's type and the runtime's `ValueExtensions`; a
   mismatch fails with `DtcgError::TokenType`.

Input references count toward `max_alias_depth`, `max_value_nodes` and the 128-level value depth
like any reference; a missing target is `DtcgError::UnresolvedReference`, and a cycle through
inputs (also an expression token whose input reaches the token itself) is an alias cycle. Tokens
that alias or point into an expression token receive its evaluated value, and an expression may
use other expression tokens as inputs.

An expression replaces the whole value; there are no expressions for members of a composite
value, and groups are never evaluated.

`ResolvedToken::snapshot` holds the resolved `$value` of every evaluated token (`None` for the
others). A value that differs from its snapshot means the authored `$value` is stale.

Every API that resolves evaluates expressions the same way: `resolve`, `project`,
`preview_source`, `preview_occurrence` and `replace_json_and_resolve_affected`. Affected
resolution follows expression inputs, so editing an input's target re-evaluates the expression
and its dependents, and an edited expression object is a changed definition. `to_document` writes
evaluated values as `$value` and keeps each token's `$extensions`, so the export resolves to the
same values with expressions on or off.

## Results

The result must be JSON: `null`, a boolean, a string, a number, an array or a plain object (whose
prototype is `Object.prototype` or `null`).

- `undefined`, functions, symbols, bigints, `NaN`, infinities and other objects (a `Map`, a
  class instance, a promise) fail with `DtcgError::Expression`, naming where in the result they
  sit (`the result at \`[3].a\` is a function, not a JSON value`).
- Results nest at most 128 levels (`LimitExceeded { kind: "value depth" }`), and their nodes count
  toward `max_value_nodes`.
- Every non-integral number is rounded to `ExpressionOptions::precision` decimal places (default
  6; `None` disables rounding). An integral number within ±2^53 becomes a JSON integer, so
  `base * 0.5` with `base` 4 is `2`, equal to an authored `2`.
- Strings are literals: a result `"{color.brand}"` is a string, not an alias.

```rust
use dxn_dtcg::{DtcgRuntime, RuntimeOptions, expressions::ExpressionOptions};

let document = r#"{
  "third": {
    "$type": "number",
    "$value": 0,
    "$extensions": { "dxn.dtcg.expressions": { "js": "1 / 3" } }
  }
}"#;
let resolve = |expressions: ExpressionOptions| -> Result<serde_json::Value, dxn_dtcg::DtcgError> {
    let options = RuntimeOptions::default().with_expressions(expressions);
    let runtime = DtcgRuntime::from_json_with_options("tokens.json", document, options)?;
    Ok(runtime.resolve(&[])?.ready().unwrap().get("third").unwrap().value.clone())
};
assert_eq!(resolve(ExpressionOptions::default())?, serde_json::json!(0.333333));
assert_eq!(
    resolve(ExpressionOptions::default().with_precision(None))?,
    serde_json::json!(1.0 / 3.0)
);
# Ok::<(), dxn_dtcg::DtcgError>(())
```

## TypeScript

A `ts` expression (and a TypeScript module) has its types removed by oxc before it runs. Types
are erased, not checked: annotations, `as`, `satisfies`, generics, interfaces and type aliases
disappear, and `enum`s compile to JavaScript.

```json
{
  "ts": "((d: { value: number; unit: string }, k: number) => ({ ...d, value: d.value * k }))(base, 1.5)",
  "inputs": { "base": "{space.base}" }
}
```

## Errors

Syntax errors and uncaught exceptions fail with `DtcgError::Expression { resource, path,
message }`. The message holds the JavaScript error and, when QuickJS reports one, its position in
the text the author wrote (for `ts`, mapped back through the removed types):

```text
expression `space.large` in `tokens.json`: ReferenceError: bas is not defined at line 1, column 12
```

Early errors that only analysis finds (a redeclared `let`, an octal literal in strict code, a
duplicate regular expression group name) are syntax errors with positions too. With the
`diagnostics` feature, the error maps to `DtcgDiagnosticCode::Expression`; the diagnostic keeps
only the error's kind and position, when there is one (`TypeError at line 2, column 9`), never its text, which may
quote document content.

## Modules

Expressions import ES modules through `imports`. The host registers JavaScript or TypeScript
modules by bare specifier:

```rust
use dxn_dtcg::{
    DtcgRuntime, RuntimeOptions,
    expressions::{ExpressionModule, ExpressionOptions},
};

let scale = ExpressionModule::typescript(
    "export const ratio: number = 1.25;\n\
     export function step(base: number, n: number): number { return base * ratio ** n; }",
);
let options = RuntimeOptions::default()
    .with_expressions(ExpressionOptions::default().with_module("brand/scale", scale));
let runtime = DtcgRuntime::from_json_with_options(
    "tokens.json",
    r#"{
      "font": {
        "$type": "number",
        "base": { "$value": 16 },
        "h1": {
          "$value": 0,
          "$extensions": { "dxn.dtcg.expressions": {
            "imports": { "brand/scale": ["step"] },
            "inputs": { "base": "{font.base}" },
            "js": "Math.round(step(base, 4))"
          } }
        }
      }
    }"#,
    options,
)?;
let tokens = runtime.resolve(&[])?.ready().unwrap();
assert_eq!(tokens.get("font.h1").unwrap().value, 39);
# Ok::<(), dxn_dtcg::DtcgError>(())
```

`"imports": { "brand/scale": ["step"] }` is `import { step } from "brand/scale"`;
`"imports": { "brand/scale": "scale" }` is `import * as scale from "brand/scale"`, which also
reaches a default export as `scale.default`.

Modules import other modules: host modules, the modules documents declare and the built-in
modules, by bare specifier. They follow standard ES module semantics: named and namespace
imports, re-exports (`export { x } from`, `export * from`), live bindings, and cycles, which
evaluate in ES module order. Only static imports exist: `import()` is rejected. Each module
compiles and its top-level code runs at most once per resolution, however many tokens import it,
and again in the next resolution, which starts fresh. Once its top-level code has run, the
objects, arrays, functions and classes (with their prototypes) the module's namespace and
top-level bindings reach are deep-frozen, so a token that tries to change a module's value gets a
`TypeError`; top-level code may build its values freely before that, but cannot change another
module's frozen values. Objects others inherit from (a function's `prototype`, an object passed
to `Object.create`) keep accepting assignments on the objects that inherit, as JavaScript's
built-ins do: `this.next = this.next.bind(this)` in a constructor still works. An object nothing
inherits from yet when it freezes stays read-only for objects created from it later; define
their properties (`Object.defineProperty`, object spread) instead of assigning them. Freezing is
charged to the module's evaluation: about 5 steps of its `step_budget` and some memory per
1,000 objects its values reach, so a module that exports a table of 100,000 objects needs a
budget of about 550 steps. Hosts provide single-file ES modules; a
library of several files is bundled beforehand.

Module failures name the module: an unknown specifier, an import of a name the module does not
export, a syntax error (with line and column), an exception thrown by a module's top-level code
(named after the module that threw, inside a cycle too), a host registration in the reserved
`dxn.dtcg` namespace, or a specifier both registered by the host and declared by a document. A
host module's failure names the token whose expression imported it; a document module's names
the module's resource id and the specifier it was imported by.

## Modules in token documents

A token document may declare modules at its root, next to its tokens, under the same extension
key:

```json
{
  "$extensions": {
    "dxn.dtcg.expressions": {
      "modules": { "palette": "lib/palette.ts" }
    }
  },
  "accent": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] } }
}
```

`modules` maps bare specifiers to references relative to the declaring document, canonicalized
with the same `ResourceId` rules as other document references (no `./` segments, no escaping the
root). The specifier must not be relative, empty or in the `dxn.dtcg` namespace, and the reference
must name a `.js`, `.mjs`, `.ts` or `.mts` file without a fragment; anything else fails with
`DtcgError::Expression` naming the document and the specifier. Declarations of several sources
combine in resolution order, and a later declaration of a specifier replaces an earlier one.

Module documents load through the same request/provide protocol as JSON documents. When an
expression imports a declared specifier, resolution returns `Resolution::Missing` with a
`DocumentRequest` whose `kind` is `DocumentKind::Module` for every module the catalog lacks:

```rust
use dxn_dtcg::{
    DocumentKind, DtcgRuntime, Resolution, RuntimeOptions, expressions::ExpressionOptions,
};

let options = RuntimeOptions::default().with_expressions(ExpressionOptions::default());
let mut runtime = DtcgRuntime::from_json_with_options(
    "themes/tokens.json",
    r#"{
      "$extensions": { "dxn.dtcg.expressions": { "modules": { "brand": "lib/brand.js" } } },
      "space": {
        "$type": "number",
        "$value": 0,
        "$extensions": { "dxn.dtcg.expressions": {
          "imports": { "brand": ["unit"] }, "js": "unit * 4"
        } }
      }
    }"#,
    options,
)?;
let modules = [
    ("themes/lib/brand.js", "export { unit } from './units.ts';"),
    ("themes/lib/units.ts", "export const unit: number = 4;"),
];
let tokens = loop {
    match runtime.resolve(&[])? {
        Resolution::Ready(tokens) => break tokens,
        Resolution::Missing(requests) => {
            for request in requests {
                assert_eq!(request.kind, DocumentKind::Module);
                let (_, source) = modules
                    .iter()
                    .find(|(id, _)| *id == request.resource_id.as_str())
                    .unwrap();
                runtime.provide_requested_module(&request, *source)?;
            }
        }
    }
};
assert_eq!(tokens.get("space").unwrap().value, 16);
# Ok::<(), dxn_dtcg::DtcgError>(())
```

- Relative imports inside a module (`./units.ts`) resolve against the module's resource id and
  are requested the same way, round by round, until the whole static import graph is loaded. The
  first round above requests `themes/lib/brand.js`, referenced from `themes/tokens.json`; the
  second requests `themes/lib/units.ts`, referenced from the module.
- A module's language follows its extension: `.ts` and `.mts` are TypeScript, `.js` and `.mjs`
  JavaScript.
- Only what expressions reach is requested: a declared module nothing imports (directly or
  through the modules expressions import) is never requested.
- `provide_module`, `provide_requested_module` and `replace_module` store module text;
  `remove_document` removes it. Each mutation is atomic: a request for a module answered with
  `provide_requested_json` (or the reverse), a module where the catalog holds JSON (or the
  reverse), or an invalid id fails and leaves the catalog unchanged. A JSON reference to a module
  document fails with `DtcgError::InvalidDocument`.
- `validate_complete` requests the modules every context's sources declare, and what they
  import, so a catalog it accepts resolves in every context.
- The preview APIs request modules like `resolve`.
- Module documents are a separate count against
  [`RuntimeLimits::max_documents`](11-errors-limits-and-diagnostics.md#runtime-limits): one
  resolution loads at most that many, besides its JSON documents (`LimitExceeded { kind:
  "documents" }`).
- Affected resolution does not follow module edits: after `replace_module`, resolve fully.

## Rust functions: native modules

A native module is a set of Rust functions and constants that expressions and modules import
like any ES module:

```rust
use dxn_dtcg::{
    DtcgRuntime, RuntimeOptions,
    expressions::{ExpressionOptions, NativeModule},
};

let math = NativeModule::new("brand/math")
    .function("double", |value: f64| value * 2.0)
    .function("ratio", |a: f64, b: f64| {
        if b == 0.0 { Err("division by zero") } else { Ok(a / b) }
    })
    .constant("golden", 1.618)
    .describe("double", "(value: number) => number", "Twice the value.");
let options = RuntimeOptions::default()
    .with_expressions(ExpressionOptions::default().with_native_module(math));
let runtime = DtcgRuntime::from_json_with_options(
    "tokens.json",
    r#"{
      "size": {
        "$type": "number",
        "$value": 0,
        "$extensions": { "dxn.dtcg.expressions": {
          "imports": { "brand/math": ["double", "ratio", "golden"] },
          "js": "double(ratio(golden * 10, 2))"
        } }
      }
    }"#,
    options,
)?;
let tokens = runtime.resolve(&[])?.ready().unwrap();
assert_eq!(tokens.get("size").unwrap().value, 16.18);
# Ok::<(), dxn_dtcg::DtcgError>(())
```

- `NativeModule::new(specifier)` names the module: a bare specifier outside the `dxn.dtcg`
  namespace that the host does not also register as a JavaScript module or a document declares.
- `.function(name, closure)` exports a closure that is `Send + Sync + 'static` and takes up to
  eight arguments of types implementing `serde::de::DeserializeOwned`. It returns a
  JSON-like value (numbers, strings, booleans, `()`, `serde_json::Value`, vectors, options,
  `BTreeMap<String, T>`,
  and the [`values` models](05-value-types.md#color-conversions)), any other serializable value
  wrapped in `expressions::Json` (`|pair: Pair| Json(pair)`), or `Result<T, E>` with any
  serializable `T` and an `E` that implements `Display`.
- Export names are JavaScript identifiers that are not reserved words (`new`, `default` and
  `delete` are not export names): `.function` and `.constant` panic on any other name.
- Arguments arrive as JSON (a missing argument is `null`, so `Option<T>` parameters are optional).
  One that does not deserialize, or contains `NaN` or an infinity (which JSON cannot carry),
  throws a `TypeError`. An `Err`, or a panic, throws an `Error`
  whose message starts with `<specifier>.<name>:`; expression code can catch it, and otherwise
  the token fails with `DtcgError::Expression`. A panic leaves the runtime usable.
- `.constant(name, value)` exports the JSON form of a serializable value.
- `.describe(name, typescript, docs)` and `.with_docs(docs)` feed the
  [TypeScript declarations](#typescript-declarations-for-editors).
- Exports are frozen, and every call serializes its arguments and result as JSON, so a native
  function never shares state with expression code. Native functions must be pure: deterministic,
  without I/O, reading only their arguments.
- `RuntimeOptions` stays `Clone`, `Debug`, `PartialEq` and `Eq`: native modules compare by
  identity (clones are equal) and print their specifier and export names.

## The `#[expression_module]` macro

`#[expression_module]` builds a native module from an `impl` block's public functions:

```rust
use dxn_dtcg::{
    DtcgRuntime, RuntimeOptions,
    expressions::{ExpressionOptions, expression_module},
    values::Dimension,
};

pub struct Spacing;

/// Spacing helpers.
#[expression_module(specifier = "brand/spacing")]
impl Spacing {
    /// The step of the spacing scale at `index`.
    pub fn scale_step(index: u32) -> f64 {
        4.0 * f64::from(index)
    }

    /// `dimension` scaled by `factor`.
    #[expression(name = "times")]
    pub fn scale(dimension: Dimension, factor: f64) -> Dimension {
        Dimension { value: dimension.value * factor, ..dimension }
    }
}

let options = RuntimeOptions::default()
    .with_expressions(ExpressionOptions::default().with_native_module(Spacing::expression_module()));
let runtime = DtcgRuntime::from_json_with_options(
    "tokens.json",
    r#"{
      "gap": {
        "$type": "dimension",
        "$value": { "value": 0, "unit": "px" },
        "$extensions": { "dxn.dtcg.expressions": {
          "imports": { "brand/spacing": ["scaleStep", "times"] },
          "js": "times({ value: scaleStep(2), unit: 'px' }, 1.5)"
        } }
      }
    }"#,
    options,
)?;
let tokens = runtime.resolve(&[])?.ready().unwrap();
assert_eq!(tokens.get("gap").unwrap().value, serde_json::json!({ "value": 12, "unit": "px" }));
# Ok::<(), dxn_dtcg::DtcgError>(())
```

- The block gains `fn expression_module() -> NativeModule`. Every `pub fn` is exported in
  camelCase (`scale_step` → `scaleStep`) unless `#[expression(name = "…")]` renames it; private
  functions stay Rust-only.
- Arguments deserialize from JSON (`&str` arrives as a `String`, `&[T]` as a `Vec<T>`, `&T` as a
  `T`); results serialize, and a function returning a `Result` fails with its error's `Display`
  text.
- Doc comments become TSDoc, and TypeScript signatures are mapped from the Rust types by their
  spelling: number types to `number`, `String`, `&str` and `char` to `string`, `bool` to
  `boolean`, `Option<T>` to `T | null`, `Vec<T>` and slices to `T[]`, `serde_json::Value` to
  `unknown`, the `values` models to their DTCG type names (`Color`, `Shadow`, `Typography` and
  the others), `()` to `void`, and anything else to `unknown`. A macro sees no trait
  implementations, so `#[expression(ts = "(value: number) => number")]` gives the signature
  where the mapping is not enough.
- A receiver (`&self`), an `async fn`, generic parameters, a generic or trait `impl` block, a
  missing `specifier`, an unknown attribute key and an export name that is not a JavaScript
  identifier or is a reserved word (`pub fn new` exports `new`; rename it with
  `#[expression(name = "…")]`) are compile errors with a message saying what
  to change.

## Built-in modules

Two native modules are always available. The `dxn.dtcg` namespace (`dxn.dtcg` and `dxn.dtcg/…`)
is reserved for them: hosts and documents cannot use it.

**`dxn.dtcg`** builds and converts DTCG values:

| Export | Result |
|---|---|
| `px(value)`, `rem(value)` | A `dimension` in `px` or `rem` |
| `dimension(value, unit)` | A `dimension`; `unit` is `px` or `rem` |
| `ms(value)`, `s(value)` | A `duration` in `ms` or `s` |
| `duration(value, unit)` | A `duration`; `unit` is `ms` or `s` |
| `toPx(dimension, remBase)` | The dimension's length in pixels, `remBase` pixels per `rem` |
| `toMillis(duration)` | The duration in milliseconds |
| `round(value, step)` | `value` rounded to the nearest multiple of `step` (default 1) |
| `clamp(value, min, max)` | `value` limited to `[min, max]` |
| `isValid(type, value)` | Whether `value` is a valid DTCG value of `type`, with the runtime's `ValueExtensions` |

**`dxn.dtcg/color`** works on DTCG color JSON with the color API of `values::Color` (see
[Value types](05-value-types.md#color-conversions)):

| Export | Result |
|---|---|
| `convert(color, space)` | The color in `space`, clipped into its ranges |
| `mix(color, other, amount?, space?)` | `other` mixed in by `amount` (default 0.5) in `space` (default `oklch`), with the shorter hue arc, premultiplied alpha and a powerless hue taking the other color's |
| `lighten(color, amount)`, `darken(color, amount)` | OKLCH lightness shifted by `amount`, in the color's space |
| `withAlpha(color, alpha)` | The color with `alpha` |
| `toHex(color)`, `fromHex(hex)` | The 6-digit sRGB hex form (`#rrggbb`), and the sRGB color of one |
| `contrast(color, other)` | The WCAG 2.x contrast ratio of the colors clipped into sRGB, from 1 to 21 |
| `inGamut(color, space)`, `clampToGamut(color, space)` | Whether the color lies in the gamut of `space` (an RGB or XYZ space, sRGB's for HSL and HWB), and the color clipped into it |

Every result is a valid DTCG color. Invalid arguments (a unit outside the type's units, a
non-number, an invalid color or color space) throw a `TypeError` naming the function, such as
`dxn.dtcg.dimension: unit \`em\` is not one of px, rem`.

```json
{
  "accent": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] } },
  "accent-hover": {
    "$type": "color",
    "$value": { "colorSpace": "srgb", "components": [0.2, 0.4, 0.9] },
    "$extensions": { "dxn.dtcg.expressions": {
      "imports": { "dxn.dtcg/color": ["lighten"] },
      "inputs": { "accent": "{accent}" },
      "js": "lighten(accent, 0.08)"
    } }
  }
}
```

## The sandbox and its limits

Expressions and modules run without ambient capabilities:

- `Date` does not exist and `Math.random` throws: there is no clock and no randomness.
- `eval` does not exist, and the `Function`, async, generator and async-generator function
  constructors throw: no code from strings.
- `Promise`, `Proxy`, `WeakRef`, `FinalizationRegistry`, timers, `queueMicrotask` and dynamic
  `import()` are unavailable. An async function returns a promise that never settles, which is
  not a JSON result.
- `globalThis`, every built-in object and prototype reachable from it or from what syntax
  creates (iterators, generators, promises), and the error stack hooks are frozen, so no
  expression can change them for another; what expressions can share is limited to module state
  (below). Assigning a member such as `constructor` or
  `toString` on your own objects still works. Inputs and native module exports are frozen too.

Each failure is a `DtcgError::Expression`, or a value when the expression catches it.

Work is bounded by `ExpressionOptions`, and expression code cannot catch the limits:

| Option | Default | Exceeding it |
|---|---|---|
| `step_budget` | 1,000 | `LimitExceeded { kind: "expression steps" }` |
| `memory_limit` | 64 MiB | `LimitExceeded { kind: "expression memory" }` |
| `stack_limit` | 256 KiB | `DtcgError::Expression` (stack overflow) |
| `precision` | `Some(6)` | — (results are rounded) |

- **Steps** are QuickJS interrupt checks, which QuickJS makes about every 10,000 function calls
  and loop iterations. Counting checks instead of time keeps the budget deterministic. Each
  evaluation (an expression call, a module's top-level code, or an import cycle's as a whole)
  has its own budget, which does not depend on what ran before it; a budget below 1 counts as 1.
- **Memory** is what the JavaScript engine of one resolution may allocate.
- **Stack** is what JavaScript code may use below the resolving call, measured from where each
  evaluation starts; at least 16 KiB. A stack overflow stops the evaluation even where code
  catches it or the engine discards it: compiled code passes every value a `catch` or `finally`
  receives, and whatever an iterator's `return` method throws while a loop or built-in closes the
  iterator, to an internal check (which unwraps `SuppressedError` and `AggregateError`); an async
  function's rejection is checked as it rejects; and the guard also stops code that allocates
  within an eighth of the limit (at most 16 KiB), so such code may fail as an overflow a little
  early. One exception remains: JavaScript discards whatever an iterator's `return` method throws
  while a loop or built-in closes the iterator after a throw, and a `return` method not written
  as one in source (installed with `Object.defineProperty`, a function defined elsewhere, a
  prototype assignment, a bound built-in) is not checked, so a stack overflow inside it is
  discarded too, and whether one occurs can depend on the remaining stack. The
  limit must leave room on the stack of the thread that resolves: one near that stack's size
  lets QuickJS overflow it before its own check, which aborts the process. Unoptimized QuickJS uses several times more stack per call, so a debug build allows much
  shallower recursion under the same limit. Build the engine optimized in every profile, as this
  repository does, in your workspace's `Cargo.toml`:
  `[profile.dev.package.quickjs-jit-sys] opt-level = 3`.
- **Module state.** A module evaluates once per resolution and every token that imports it
  shares it. Its namespace and top-level values are deep-frozen once its top-level code has run,
  but freezing cannot reach all of its state, and that state is shared within the resolution
  whichever code changes it, token expressions included: module-level variables its functions
  reassign, variables a closure keeps, and the contents of the `Map`s, `Set`s, typed arrays,
  `ArrayBuffer`s and `DataView`s it holds. Modules may hold and export such values; keep module
  functions pure and do not write into a module's collections from expressions. Results that
  depend on such writes are guaranteed neither independent of the order tokens evaluate in nor
  equal between `apply_affected` and a full resolution.
- **Nesting.** The JavaScript and TypeScript compiler works recursively on the caller's stack,
  so every source (an expression or a module) may nest only so deeply: about 120 nested
  brackets, 40 nested function bodies or 300 chained operators, much more than real code uses,
  far less than overflows a 1 MiB stack. Deeper text fails with
  `DtcgError::Expression` ("the source nests too deeply"); regular expression literals nest at
  most 20 groups and classes, and so do patterns built at run time (`new RegExp`, and strings
  given to `match`, `matchAll` and `search`): a deeper one throws a `SyntaxError` ("the pattern
  nests too deeply"), so the regular expression compiler never runs out of stack.
  `RegExp.prototype.compile` is not available.

After any limit, the same runtime resolves again normally. A resolution without expression
tokens never starts the JavaScript engine at all.

## TypeScript declarations for editors

`ExpressionOptions::typescript_declarations()` returns one `.d.ts` text that declares the DTCG
value types (`Color`, `Dimension`, `Duration`, `CubicBezier`, `Shadow`, `Typography`, …), the
built-in modules and every registered native module, with their documentation. Editors that load
it type-check and complete expressions and modules:

```rust
use dxn_dtcg::expressions::{ExpressionOptions, NativeModule};

let options = ExpressionOptions::default().with_native_module(
    NativeModule::new("brand/math")
        .function("double", |value: f64| value * 2.0)
        .describe("double", "(value: number) => number", "Twice the value."),
);
let declarations = options.typescript_declarations();
assert!(declarations.contains("declare module \"dxn.dtcg/color\""));
assert!(declarations.contains("export const double: (value: number) => number;"));
```

JavaScript and TypeScript modules are not declared: their sources carry their own types. A
native function registered without `describe` is declared as `(...args: unknown[]) => unknown`.

## WebAssembly

With the feature, `dxn_dtcg` builds for `wasm32-unknown-unknown`. QuickJS is C, so the build
needs an LLVM clang with the WebAssembly target and `llvm-ar`, named through the `cc` crate's
variables:

```sh
CC_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/clang \
AR_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/llvm-ar \
cargo build --target wasm32-unknown-unknown --features expressions
```

Apple's clang has no WebAssembly target; Homebrew's `llvm` or a distribution's `clang` and
`llvm` packages do. The default build needs no C compiler at all.

- The module needs no host import on QuickJS's behalf. QuickJS's WebAssembly shim imports a clock
  from the host as `env.__rquickjs_host_now_us`; the crate defines that symbol itself, returning
  0, because expressions have no clock. The symbol belongs to the whole WebAssembly module, which
  links one copy of QuickJS: other code in the same module that uses `quickjs-jit` directly reads
  this clock too (`Date.now()` is 0) and cannot supply its own `env.__rquickjs_host_now_us`
  (defining it again fails to link).
- The default `stack_limit` (256 KiB) fits the default 1 MiB WebAssembly stack.
- Panics abort on `wasm32-unknown-unknown`, so a panicking native function stops the module
  instead of becoming a JavaScript error.

## Determinism

Equal documents, selections and options give equal `ResolvedTokens` and equal fingerprints, in any
evaluation order unless results depend on writes to shared module state (below): each resolution starts a fresh, frozen engine, expressions cannot share state
through globals or built-ins, and inputs are frozen. Rounding results to `precision` keeps them
equal across platforms whose math libraries differ in the last bit. A module evaluates once per
resolution and its tokens share it, so its values freeze once its top-level code has run (see
[modules](#modules)): no token can change a plain object, array, function or class another token
reads. State freezing cannot reach is shared within the resolution, whoever writes it ([module
state](#the-sandbox-and-its-limits)); results that depend on such writes can differ between
evaluation orders and between affected and full resolution.

## What expressions do not do

- Type-check TypeScript (types are removed, not checked) or declare per-token input types.
- Run async code, promises, timers or dynamic `import()`.
- Bundle npm packages or load modules from the file system or the network: hosts provide
  single-file ES modules as strings, bundling multi-file packages beforehand.
- Follow module edits in affected resolution, or rewrite the authored `$value` snapshots.
- Cache compiled modules across resolution calls.
