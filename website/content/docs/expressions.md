---
title: Expressions
description: Computed token values with JavaScript and TypeScript in the dxn.dtcg.expressions extension - enabling them, evaluation, modules, Rust functions, the built-in dxn.dtcg and dxn.dtcg/color modules, the sandbox, its limits and WebAssembly.
group: Guides
order: 135
---

Expressions compute a token's value with JavaScript or TypeScript. A spacing scale, a hover color or a type ramp can be derived from other tokens instead of copied by hand, and the derivation lives in the token document next to the value it produces. Expressions are opt-in twice: the `expressions` Cargo feature compiles the engine in, and `RuntimeOptions::with_expressions` turns evaluation on for one runtime.

## Why an extension

DTCG 2025.10 has no computed values. Format §8 requires a token's `$value` to match its `$type`: a dimension is `{ "value": 16, "unit": "px" }`, never a formula. Format §5.2.3 lets tools put their own data under `$extensions` and requires other tools to preserve it, while recommending that it stay optional metadata. The community group closed computed values as out of scope for the format, so an extension is the route the specification leaves open.

An expression therefore lives in the token's `$extensions`, under the key `dxn.dtcg.expressions` (the constant `dxn_dtcg::EXPRESSION_EXTENSION`), and the token keeps a valid `$value`:

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

The `$value` is the **snapshot**: what every DTCG tool that ignores the extension reads, including this runtime with expressions off. With expressions on, `dxn_dtcg` evaluates the expression and uses its result instead. A document with expressions is a valid DTCG 2025.10 document either way.

## Enabling expressions

The Cargo feature `expressions` compiles the JavaScript engine (QuickJS, through the `quickjs-jit` crate) and the TypeScript compiler (oxc) in. It is off by default:

```toml
[dependencies]
dxn_dtcg = { git = "https://github.com/dXnSix/dXn.Dtcg", tag = "vX.Y.Z", features = ["expressions"] }
```

Use a release tag that includes the feature. Then `RuntimeOptions::with_expressions` turns evaluation on for one runtime:

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, RuntimeOptions, expressions::ExpressionOptions};
use serde_json::json;

fn main() -> Result<(), DtcgError> {
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
    assert_eq!(large.value, json!({ "value": 20, "unit": "px" }));
    // The authored `$value`, resolved, is kept as the snapshot.
    assert_eq!(large.snapshot, Some(json!({ "value": 16, "unit": "px" })));
    Ok(())
}
```

Without the option, the same runtime resolves `space.large` to its snapshot, keeps the extension object unchanged in `ResolvedToken::extensions`, and never validates it. The option, not the feature, decides: when Cargo unifies features and one crate of a build enables `expressions`, another crate's runtime still resolves exactly as before.

:::note What the feature costs
The default build compiles no C code and contains no JavaScript engine. The feature adds QuickJS's C sources, built with the platform C compiler, and oxc. See [Platforms and versions](./platforms.md#crates-and-features).
:::

## The expression object

The object at `$extensions["dxn.dtcg.expressions"]` of a token has these members:

| Member | Required | Value |
|---|---|---|
| `js` or `ts` | exactly one | One JavaScript or TypeScript expression (not statements), as a string |
| `inputs` | no | An object from input names to DTCG values: literals, `{curly}` aliases or JSON Pointer `$ref` objects, also nested inside objects and arrays |
| `imports` | no | An object from module specifiers to an array of export names (named imports) or to one name (a namespace import) |

Rules, checked only when expressions are enabled:

- The object has no other members; `js` and `ts` are strings.
- Input names and imported names are JavaScript identifiers (ECMAScript identifier syntax, without escapes) that are not reserved words in an ES module, including `await`, `eval` and `arguments`. Names are unique across `inputs` and `imports`.
- Import specifiers are bare (`"brand/math"`), not relative (`"./math.js"`) and not empty.
- `js` and `ts` hold exactly one expression: text that would close the generated function early (`"1), (2"`) is a syntax error. Use an arrow function to run statements: `"(() => { const x = base.value * 2; return x + 1; })()"`.
- The token still needs `$value` (or a token-level `$ref`), as every DTCG token does.

A violation fails with `DtcgError::Expression { resource, path, message }`, naming the token and the member. With expressions off the object is opaque: even a malformed one is kept and ignored.

## How resolution evaluates an expression

A token with an expression resolves in two steps:

1. Its `$value` resolves exactly as without expressions: references, the [effective type](./aliases.md#types-through-references) (own `$type`, then the type a whole-value reference targets, then the group's) and validation. An invalid snapshot is still an error, and the token has the same type with expressions on or off.
2. Its `inputs` resolve like token values at a position without type rules: aliases, JSON Pointers (also into document locations such as `#/$extensions/...`) and literals. The expression then runs with each input bound by name to a frozen JSON value. Its result replaces the value and is validated against the token's type and the runtime's `ValueExtensions`; a mismatch fails with `DtcgError::TokenType`.

Input references count toward `max_alias_depth`, `max_value_nodes` and the 128-level value depth like any reference. A missing target is `DtcgError::UnresolvedReference`, and a cycle through inputs (including an input that reaches the token itself) is an alias cycle. Tokens that alias or point into an expression token receive its evaluated value, and an expression may use other expression tokens as inputs.

An expression replaces the whole value: there are no expressions for members of a composite value, and groups are never evaluated.

`ResolvedToken::snapshot` holds the resolved `$value` of every evaluated token and is `None` for the others. A value that differs from its snapshot means the authored `$value` is stale.

Every API that resolves evaluates expressions the same way: `resolve`, `project`, `preview_source`, `preview_occurrence` and `replace_json_and_resolve_affected`. Affected resolution follows expression inputs, so editing an input's target re-evaluates the expression and its dependents, and an edited expression object is a changed definition ([Incremental resolution](./incremental.md)). `to_document` writes evaluated values as `$value` and keeps each token's `$extensions`, so the export resolves to the same values with expressions on or off.

## Results

The result must be JSON: `null`, a boolean, a string, a number, an array or a plain object (whose prototype is `Object.prototype` or `null`).

- `undefined`, functions, symbols, bigints, `NaN`, infinities and other objects (a `Map`, a class instance, a promise) fail with `DtcgError::Expression`, naming where in the result they sit (``the result at `[3].a` is a function, not a JSON value``).
- Results nest at most 128 levels (`LimitExceeded { kind: "value depth" }`), and their nodes count toward `max_value_nodes`.
- Every non-integral number is rounded to `ExpressionOptions::precision` decimal places (default 6; `None` disables rounding). An integral number within ±2^53 becomes a JSON integer, so `base * 0.5` with `base` 4 is `2`, equal to an authored `2`.
- Strings are literals: a result `"{color.brand}"` is a string, not an alias.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, RuntimeOptions, expressions::ExpressionOptions};
use serde_json::{Value, json};

const TOKENS: &str = r#"{
  "third": {
    "$type": "number",
    "$value": 0,
    "$extensions": { "dxn.dtcg.expressions": { "js": "1 / 3" } }
  }
}"#;

fn third(expressions: ExpressionOptions) -> Result<Value, DtcgError> {
    let options = RuntimeOptions::default().with_expressions(expressions);
    let runtime = DtcgRuntime::from_json_with_options("tokens.json", TOKENS, options)?;
    Ok(runtime.resolve(&[])?.ready().unwrap().get("third").unwrap().value.clone())
}

fn main() -> Result<(), DtcgError> {
    assert_eq!(third(ExpressionOptions::default())?, json!(0.333333));
    assert_eq!(third(ExpressionOptions::default().with_precision(None))?, json!(1.0 / 3.0));
    Ok(())
}
```

## TypeScript

A `ts` expression (and a TypeScript module) has its types removed by oxc before it runs. Types are erased, not checked: annotations, `as`, `satisfies`, generics, interfaces and type aliases disappear, and `enum`s compile to JavaScript.

```json
{
  "ts": "((d: { value: number; unit: string }, k: number) => ({ ...d, value: d.value * k }))(base, 1.5)",
  "inputs": { "base": "{space.base}" }
}
```

## Errors

Syntax errors and uncaught exceptions fail with `DtcgError::Expression { resource, path, message }`. The message holds the JavaScript error and, when QuickJS reports one, its position in the text the author wrote; for `ts`, the position is mapped back through the removed types:

```text
expression `space.large` in `tokens.json`: ReferenceError: bas is not defined at line 1, column 12
```

Early errors that only analysis finds (a redeclared `let`, an octal literal in strict code, a duplicate regular expression group name) are syntax errors with positions too. With the `diagnostics` feature, the error maps to `DtcgDiagnosticCode::Expression`, and the diagnostic's message keeps only the error's kind and position, when there is one (`TypeError at line 2, column 9`), never its text, which may quote document content. See [Errors](./errors.md#expression-errors).

## Modules

Expressions import ES modules through `imports`. The host registers JavaScript or TypeScript modules by bare specifier with `ExpressionOptions::with_module` and `ExpressionModule::javascript` or `ExpressionModule::typescript`:

```rust
use dxn_dtcg::{
    DtcgError, DtcgRuntime, RuntimeOptions,
    expressions::{ExpressionModule, ExpressionOptions},
};

fn main() -> Result<(), DtcgError> {
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
    Ok(())
}
```

`"imports": { "brand/scale": ["step"] }` is `import { step } from "brand/scale"`; `"imports": { "brand/scale": "scale" }` is `import * as scale from "brand/scale"`, which also reaches a default export as `scale.default`.

- Modules import other modules by bare specifier: host modules, the modules documents declare and the built-in modules. A host module cannot import relatively; a document module can (see below).
- Standard ES module semantics apply: named and namespace imports, re-exports (`export { x } from`, `export * from`), live bindings, and cycles, which evaluate in ES module order. Only static imports exist: `import()` is rejected.
- Each module compiles and its top-level code runs at most once per resolution, however many tokens import it, and again in the next resolution, which starts fresh.
- Hosts provide single-file ES modules; a library of several files is bundled beforehand.

Module failures name the module: an unknown specifier, an import of a name the module does not export, a syntax error (with line and column), an exception thrown by a module's top-level code (named after the module that threw, inside a cycle too), a host registration in the reserved `dxn.dtcg` namespace, or a specifier both registered by the host and declared by a document. A host module's failure names the token whose expression imported it; a document module's names the module's resource id and the specifier it was imported by.

## Modules in token documents

A token document may declare modules at its root, next to its tokens, under the same extension key:

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

- `modules` is the only member of the root object. It maps bare specifiers to references relative to the declaring document, canonicalized with the same [resource id rules](./loading.md#resource-ids) as other document references.
- The specifier must not be relative, empty or in the `dxn.dtcg` namespace, and the reference must name a `.js`, `.mjs`, `.ts` or `.mts` file without a fragment. Anything else fails with `DtcgError::Expression` naming the document and the specifier.
- Only the root of a whole document declares modules; a fragment source or an inline source declares none.
- Declarations of several sources combine in resolution order, and a later declaration of a specifier replaces an earlier one.

Module documents load through the same [request/provide protocol](./loading.md#module-documents) as JSON documents. When an expression imports a declared specifier, resolution returns `Resolution::Missing` with a `DocumentRequest` whose `kind` is `DocumentKind::Module` for every module the catalog lacks, and you answer with `provide_requested_module`:

```rust
use dxn_dtcg::{
    DocumentKind, DtcgError, DtcgRuntime, Resolution, RuntimeOptions,
    expressions::ExpressionOptions,
};

fn main() -> Result<(), DtcgError> {
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
    Ok(())
}
```

- Relative imports inside a module (`./units.ts`) resolve against the module's resource id and are requested the same way, round by round, until the whole static import graph is loaded. Above, the first round requests `themes/lib/brand.js`, referenced from `themes/tokens.json`; the second requests `themes/lib/units.ts`, referenced from the module.
- A module's language follows its extension: `.ts` and `.mts` are TypeScript, `.js` and `.mjs` JavaScript.
- Only what expressions reach is requested: a declared module nothing imports, directly or through the modules expressions import, is never requested.
- The preview APIs request modules like `resolve`, and `validate_complete` requests the modules every context's sources declare, and what they import, so a catalog it accepts resolves in every context.
- Affected resolution does not follow module edits: after `replace_module`, resolve fully.

The catalog methods for modules (`provide_module`, `provide_requested_module`, `replace_module`, and `remove_document` for removal) and their atomicity are described in [Loading documents](./loading.md#module-documents).

## Rust functions: native modules

A native module is a set of Rust functions and constants that expressions and modules import like any ES module:

```rust
use dxn_dtcg::{
    DtcgError, DtcgRuntime, RuntimeOptions,
    expressions::{ExpressionOptions, NativeModule},
};

fn main() -> Result<(), DtcgError> {
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
    Ok(())
}
```

- `NativeModule::new(specifier)` names the module: a bare specifier outside the `dxn.dtcg` namespace that the host does not also register as a JavaScript module and no document declares.
- `.function(name, closure)` exports a closure that is `Send + Sync + 'static` and takes up to eight arguments whose types implement `serde::de::DeserializeOwned` (the `NativeFunction` trait). It returns a `NativeReturn`: a JSON-like value (numbers, `char`, strings, booleans, `()`, `serde_json::Value`, `Vec<T>`, arrays, `Option<T>`, `BTreeMap<String, T>`, and the twelve [`values` models](./value-types.md#color-conversions)), any other serializable value wrapped in `expressions::Json` (`|pair: Pair| Json(pair)`), or `Result<T, E>` with any serializable `T` and an `E` that implements `Display`.
- Export names are JavaScript identifiers that are not reserved words (`new`, `default` and `delete` are not export names): `.function` and `.constant` panic on any other name.
- Arguments arrive as JSON; a missing argument is `null`, so `Option<T>` parameters are optional. An argument that does not deserialize throws a `TypeError`. An `Err`, or a panic, throws an `Error` whose message starts with `<specifier>.<name>:`; expression code can catch it, and otherwise the token fails with `DtcgError::Expression`. A panic leaves the runtime usable.
- `.constant(name, value)` exports the JSON form of a serializable value.
- `.describe(name, typescript, docs)` and `.with_docs(docs)` feed the [TypeScript declarations](#typescript-declarations-for-editors).
- Exports are frozen, and every call serializes its arguments and result as JSON, so a native function never shares state with expression code. Native functions must be pure: deterministic, without I/O, reading only their arguments.
- `RuntimeOptions` stays `Clone`, `Debug`, `PartialEq` and `Eq`: native modules compare by identity (clones are equal), and their `Debug` output prints the specifier and the export names.

## The `#[expression_module]` macro

`#[expression_module]` builds a native module from the public functions of an `impl` block:

```rust
use dxn_dtcg::{
    DtcgError, DtcgRuntime, RuntimeOptions,
    expressions::{ExpressionOptions, expression_module},
    values::Dimension,
};
use serde_json::json;

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

fn main() -> Result<(), DtcgError> {
    let expressions = ExpressionOptions::default().with_native_module(Spacing::expression_module());
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
        RuntimeOptions::default().with_expressions(expressions),
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();
    assert_eq!(tokens.get("gap").unwrap().value, json!({ "value": 12, "unit": "px" }));
    Ok(())
}
```

- The block gains `fn expression_module() -> NativeModule`. Every `pub fn` is exported in camelCase (`scale_step` becomes `scaleStep`) unless `#[expression(name = "...")]` renames it; private functions stay Rust-only.
- Arguments deserialize from JSON (`&str` arrives as a `String`, `&[T]` as a `Vec<T>`, `&T` as a `T`); results serialize, and a function returning a `Result` fails with its error's `Display` text.
- Doc comments become TSDoc, and TypeScript signatures are mapped from the Rust types by their spelling: number types to `number`; `String`, `&str` and `char` to `string`; `bool` to `boolean`; `Option<T>` to `T | null`; `Vec<T>` and slices to `T[]`; `serde_json::Value` to `unknown`; the `values` models to their DTCG type names (`Color`, `Shadow`, `Typography` and the others); `()` to `void`; anything else to `unknown`. A macro sees no trait implementations, so `#[expression(ts = "(value: number) => number")]` gives the signature where the mapping is not enough.
- A receiver (`&self`), an `async fn`, generic parameters, a generic or trait `impl` block, a missing `specifier`, an unknown attribute key and an export name that is not a JavaScript identifier or is a reserved word (`pub fn new` would export `new`; rename it with `#[expression(name = "...")]`) are compile errors that say what to change.

## Built-in modules

Two native modules are always available. The `dxn.dtcg` namespace (`dxn.dtcg` and `dxn.dtcg/...`) is reserved for them: hosts and documents cannot use it.

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

**`dxn.dtcg/color`** works on DTCG color JSON with the [color API of `values::Color`](./value-types.md#color-conversions):

| Export | Result |
|---|---|
| `convert(color, space)` | The color in `space`, clipped into its ranges |
| `mix(color, other, amount?, space?)` | `other` mixed in by `amount` (default 0.5) in `space` (default `oklch`), with the shorter hue arc, premultiplied alpha and a powerless hue taking the other color's |
| `lighten(color, amount)`, `darken(color, amount)` | OKLCH lightness shifted by `amount`, in the color's space |
| `withAlpha(color, alpha)` | The color with `alpha` |
| `toHex(color)`, `fromHex(hex)` | The 6-digit sRGB hex form (`#rrggbb`), and the sRGB color of one |
| `contrast(color, other)` | The WCAG 2.x contrast ratio, from 1 to 21 |
| `inGamut(color, space)`, `clampToGamut(color, space)` | Whether the color lies in the gamut of `space` (an RGB or XYZ space, sRGB's for HSL and HWB), and the color clipped into it |

Every color result is a valid DTCG color. Invalid arguments (a unit outside the type's units, a non-number, an invalid color or color space) throw a `TypeError` naming the function, such as ``dxn.dtcg.dimension: unit `em` is not one of px, rem``.

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
- `eval` does not exist, and the `Function`, async, generator and async-generator function constructors throw: no code from strings.
- `Promise`, `Proxy`, `WeakRef`, `FinalizationRegistry`, timers, `queueMicrotask` and dynamic `import()` are unavailable. An async function returns a promise that never settles, which is not a JSON result.
- `globalThis`, every built-in object and prototype reachable from it or from what syntax creates (iterators, generators, promises), and the error stack hooks are frozen, so no expression can change what another sees. Assigning a member such as `constructor` or `toString` on your own objects still works. Inputs and native module exports are frozen too.

Each failure is a `DtcgError::Expression`, or a value when the expression catches it.

Work is bounded by `ExpressionOptions` (`#[non_exhaustive]`; start from `default()` and use the `with_*` setters), and expression code cannot catch the limits:

| Option | Setter | Default | Exceeding it |
|---|---|---|---|
| `step_budget` | `with_step_budget` | 1,000 | `LimitExceeded { kind: "expression steps" }` |
| `memory_limit` | `with_memory_limit` | 64 MiB | `LimitExceeded { kind: "expression memory" }` |
| `stack_limit` | `with_stack_limit` | 256 KiB | `DtcgError::Expression` (stack overflow) |
| `precision` | `with_precision` | `Some(6)` | none: results are rounded |

- **Steps** are QuickJS interrupt checks, which QuickJS makes about every 10,000 function calls and loop iterations. Counting checks instead of time keeps the budget deterministic. Each evaluation (an expression call, a module's top-level code, or an import cycle's as a whole) has its own budget, which does not depend on what ran before it; a budget below 1 counts as 1.
- **Memory** is what the JavaScript engine of one resolution may allocate.
- **Stack** is what JavaScript code may use below the resolving call; at least 16 KiB. The limit must leave room on the stack of the thread that resolves: a limit near that stack's size lets QuickJS overflow it before its own check, which aborts the process.
- **Nesting.** The JavaScript and TypeScript compiler works recursively on the caller's stack, so every source (an expression or a module) may nest only so deeply: about 120 nested brackets, 40 nested function bodies or 300 chained operators. That is much more than real code uses and far less than overflows a 1 MiB stack. Deeper text fails with `DtcgError::Expression` (`the source nests too deeply`); regular expression literals nest at most 20 groups.

After any limit, the same runtime resolves again normally. A resolution without expression tokens never starts the JavaScript engine. [Limits and diagnostics](./limits.md#expression-limits) lists these bounds next to the runtime limits.

## TypeScript declarations for editors

`ExpressionOptions::typescript_declarations()` returns one `.d.ts` text that declares the DTCG value types (`Color`, `Dimension`, `Duration`, `CubicBezier`, `Shadow`, `Typography`, ...), the built-in modules and every registered native module, with their documentation. Editors that load it type-check and complete expressions and modules:

```rust
use dxn_dtcg::expressions::{ExpressionOptions, NativeModule};

fn main() {
    let options = ExpressionOptions::default().with_native_module(
        NativeModule::new("brand/math")
            .function("double", |value: f64| value * 2.0)
            .describe("double", "(value: number) => number", "Twice the value."),
    );
    let declarations = options.typescript_declarations();
    assert!(declarations.contains("declare module \"dxn.dtcg/color\""));
    assert!(declarations.contains("export const double: (value: number) => number;"));
}
```

JavaScript and TypeScript modules are not declared: their sources carry their own types. A native function registered without `describe` is declared as `(...args: unknown[]) => unknown`.

## WebAssembly

With the feature, `dxn_dtcg` builds for `wasm32-unknown-unknown`. QuickJS is C, so the build needs an LLVM clang with the WebAssembly target and `llvm-ar`, named through the `cc` crate's variables:

```sh
CC_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/clang \
AR_wasm32_unknown_unknown=/opt/homebrew/opt/llvm/bin/llvm-ar \
cargo build --target wasm32-unknown-unknown --features expressions
```

Apple's clang has no WebAssembly target; Homebrew's `llvm` or a distribution's `clang` and `llvm` packages do. The default build needs no C compiler at all.

- **No QuickJS host import.** QuickJS's WebAssembly shim imports a clock from the host as `env.__rquickjs_host_now_us`. The crate defines that symbol itself, always returning 0, because expressions have no clock, so the module needs no host import on QuickJS's behalf.
- **The clock symbol is module-wide.** The symbol belongs to the whole WebAssembly module, which links one copy of QuickJS. Other code in the same module that uses `quickjs-jit` directly reads this clock too (`Date.now()` is 0) and cannot supply its own `env.__rquickjs_host_now_us`: defining it again fails to link.
- The default `stack_limit` (256 KiB) fits the default 1 MiB WebAssembly stack.
- Panics abort on `wasm32-unknown-unknown`, so a panicking native function stops the module instead of becoming a JavaScript error.

CI builds and tests this configuration; see [Platforms and versions](./platforms.md#webassembly-and-expressions).

## Determinism

Equal documents, selections and options give equal `ResolvedTokens` and equal fingerprints, in any evaluation order: each resolution starts a fresh, frozen engine, expressions cannot share state through globals or built-ins, and inputs are frozen. Rounding results to `precision` keeps them equal across platforms whose math libraries differ in the last bit. Module-level variables are shared by the tokens that import the module within one resolution, so modules should keep no state that tokens change.

## What expressions do not do

- Type-check TypeScript (types are removed, not checked) or declare per-token input types.
- Run async code, promises, timers or dynamic `import()`.
- Bundle npm packages or load modules from the file system or the network: hosts provide single-file ES modules as strings, bundling multi-file packages beforehand.
- Follow module edits in affected resolution, or rewrite the authored `$value` snapshots.
- Cache compiled modules across resolution calls.

Next: [Errors](./errors.md), or [Limits and diagnostics](./limits.md).
