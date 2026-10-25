# 4. Aliases and references

Tokens can take their values from other tokens. DTCG 2025.10 has two syntaxes for this, and
`dxn_dtcg` supports both: curly-brace aliases as whole values and at the composite members the
format allows, and JSON Pointer references at any depth.

- [The two syntaxes](#the-two-syntaxes)
- [Curly-brace aliases](#curly-brace-aliases)
- [Where curly-brace aliases are allowed](#where-curly-brace-aliases-are-allowed)
- [Types through references](#types-through-references)
- [JSON Pointer references](#json-pointer-references)
- [Pointer syntax](#pointer-syntax)
- [Token-level `$ref`](#token-level-ref)
- [References to arbitrary locations](#references-to-arbitrary-locations)
- [Adjusting a referenced value](#adjusting-a-referenced-value)
- [How a pointer is interpreted](#how-a-pointer-is-interpreted)
- [Aliases see the final token set](#aliases-see-the-final-token-set)
- [`authored_value` and `value`](#authored_value-and-value)
- [Cycles, depth and errors](#cycles-depth-and-errors)

## The two syntaxes

| Syntax | Example | Refers to |
|---|---|---|
| Curly-brace alias | `"{color.brand.primary}"` | The resolved value of a token, by token path |
| JSON Pointer reference | `{ "$ref": "#/color/brand/primary/$value" }` | A location in the document: a token's value or type, a part of a value, or any other JSON |

Both are resolved after all sources of the selection are merged into one document (see
[Token documents](03-token-documents.md#several-sources-one-merged-document)). A curly-brace alias
may be the whole value or sit at one of the composite positions listed
[below](#where-curly-brace-aliases-are-allowed). A JSON Pointer reference may appear anywhere inside
a value.

## Curly-brace aliases

A JSON string that **starts with `{` and ends with `}`** is an alias. The text between the braces
is a token path.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "color": {
            "$type": "color",
            "border": { "$value": { "colorSpace": "srgb", "components": [0.8, 0.82, 0.86] } },
            "shadow": { "$value": { "colorSpace": "srgb", "components": [0, 0, 0], "alpha": 0.2 } }
          },
          "size": {
            "$type": "dimension",
            "hairline": { "$value": { "value": 1, "unit": "px" } },
            "none": { "$value": { "value": 0, "unit": "px" } },
            "blur": { "$value": { "value": 6, "unit": "px" } },
            "body": { "$value": { "value": 16, "unit": "px" } }
          },
          "font": {
            "body": { "$type": "fontFamily", "$value": ["Inter", "system-ui", "sans-serif"] },
            "regular": { "$type": "fontWeight", "$value": 400 },
            "leading": { "$type": "number", "$value": 1.5 }
          },

          "card": {
            "border": {
              "$type": "border",
              "$value": { "color": "{color.border}", "width": "{size.hairline}", "style": "solid" }
            },
            "shadow": {
              "$type": "shadow",
              "$value": [{
                "color": "{color.shadow}", "offsetX": "{size.none}", "offsetY": "{size.hairline}",
                "blur": "{size.blur}", "spread": "{size.none}"
              }]
            },
            "text": {
              "$type": "typography",
              "$value": {
                "fontFamily": "{font.body}", "fontSize": "{size.body}", "fontWeight": "{font.regular}",
                "letterSpacing": "{size.none}", "lineHeight": "{font.leading}"
              }
            },
            "outline": { "$value": "{card.border}" }
          }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    let border = &tokens.get("card.border").unwrap().value;
    assert_eq!(border["width"]["value"], 1);
    assert_eq!(border["color"]["components"][0], 0.8);

    let shadow = &tokens.get("card.shadow").unwrap().value;
    assert_eq!(shadow[0]["color"]["alpha"], 0.2);

    let text = &tokens.get("card.text").unwrap().value;
    assert_eq!(text["fontFamily"][0], "Inter");
    assert_eq!(text["lineHeight"], 1.5);

    // A whole-value alias also takes the target's type.
    let outline = tokens.get("card.outline").unwrap();
    assert_eq!(outline.token_type.as_deref(), Some("border"));
    assert_eq!(outline.value, *border);
    Ok(())
}
```

Rules:

- **The whole string is the alias.** There is no interpolation: `"x {a}"` is the literal string
  `x {a}`. Conversely, a value such as `"{Inter}"` is always read as an alias, here to a token
  named `Inter`, and fails if there is none.
- **The target must be a token.** `{spacing}` names a group and fails with
  `DtcgError::UnresolvedReference` (``unresolved reference `spacing`: missing alias target``), as
  does the empty alias `{}`. Write `{spacing.$root}` to refer to a group's root token.
- **Targets are looked up in the final token set** of the current selection, whichever source
  defined them (see [below](#aliases-see-the-final-token-set)).
- **The resolved value is inserted as is.** An alias is replaced by its target's resolved value; an
  array-valued target is never spliced into a surrounding array.

## Where curly-brace aliases are allowed

DTCG 2025.10 (Format §7 and §9) allows an alias as a token's whole `$value` and at the members of
composite types. The runtime enforces exactly that. Inside a value, an alias is allowed only at
these positions, and it must reference a token of the listed type:

| Token type | Position | Target type |
|---|---|---|
| `border` | `color` / `width` / `style` | `color` / `dimension` / `strokeStyle` |
| `transition` | `duration`, `delay` / `timingFunction` | `duration` / `cubicBezier` |
| `shadow` | `color` / `offsetX`, `offsetY`, `blur`, `spread` | `color` / `dimension` |
| `shadow` | an element of a shadow array | `shadow` |
| `gradient` | a stop's `color` / `position` | `color` / `number` |
| `gradient` | an element of the gradient array | `gradient` |
| `typography` | `fontFamily` / `fontSize`, `letterSpacing` / `fontWeight` / `lineHeight` | `fontFamily` / `dimension` / `fontWeight` / `number` (also `dimension` with the line-height value extension) |
| `strokeStyle` (also as a border `style`) | an element of `dashArray` | `dimension` |
| `dxn.dtcg.blur` (value extension) | `radius` | `dimension` |

Everywhere else an alias fails with `DtcgError::Token` and the message
``curly-brace references are not allowed inside <type> values``. That covers color `colorSpace`,
`components`, `alpha` and `hex`; the members of a dimension or duration; `cubicBezier` elements;
the elements of a `fontFamily` array; `lineCap`; shadow `inset`; and a whole `dashArray`. An alias
at an allowed position whose target has another type fails with `DtcgError::TokenType` and
``reference type mismatch: expected <type>, found <type>``.

Because an alias's value is inserted whole, an element alias only produces a valid value when the
target's value fits there: a shadow array element can alias a single-shadow token, but not a
token whose value is itself an array.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn resolve(json: &str) -> Result<(), DtcgError> {
    DtcgRuntime::from_json("tokens.json", json)?.resolve(&[]).map(|_| ())
}

fn main() {
    // An alias inside a fontFamily array is not a composite position.
    let error = resolve(
        r#"{
          "font": { "$type": "fontFamily", "brand": { "$value": "Inter" } },
          "stack": { "$type": "fontFamily", "$value": ["{font.brand}", "sans-serif"] }
        }"#,
    )
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "token `stack` in `tokens.json`: curly-brace references are not allowed inside fontFamily values"
    );

    // A border width must alias a dimension token.
    let error = resolve(
        r#"{
          "ink": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [0, 0, 0] } },
          "one": { "$type": "number", "$value": 1 },
          "rule": { "$type": "border", "$value": { "color": "{ink}", "width": "{one}", "style": "solid" } }
        }"#,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        DtcgError::TokenType { ref path, ref message, .. }
            if path == "rule" && message == "reference type mismatch: expected dimension, found number"
    ));
}
```

The positions follow the token's own `$type` or, failing that, its group's `$type`. The members
of a token that has neither are not position-checked, and JSON found through a pointer is never
position-checked; the resolved value is still validated against the type the token ends up with.
To reuse part of a value at a position that does not admit aliases, use a
[JSON Pointer reference](#json-pointer-references).

## Types through references

Every token gets exactly one effective type. Following Format §5.2.2, the runtime takes the first
of:

1. the token's own `$type`;
2. the resolved type of the token that a **whole-value reference** targets. A whole-value reference
   is a curly-brace alias, a `{ "$ref": "#/…/$value" }` object with no other members, or a
   [token-level `$ref`](#token-level-ref) to `…/$value`, where the part before `/$value` is a token
   path;
3. the type of the nearest enclosing group, after sources are merged and `$extends` is applied
   (see [Token documents](03-token-documents.md#types-and-inheritance));
4. otherwise the token fails with `TokenType`: `token requires a DTCG type`.

So an untyped alias in a `color` group that references a `dimension` token is a `dimension` token.
Aliases nested inside a value do not affect the token's type.

When a token has its own `$type` **and** a whole-value reference, the two must name the same type.
Otherwise resolution fails with `TokenType`: ``reference type mismatch: expected <own>, found
<target>``. The comparison is by type name, not by shape: a `fontWeight` token that aliases a
`number` token holding `400` fails, even though `400` is a valid font weight.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "size": { "$type": "dimension", "sm": { "$value": { "value": 4, "unit": "px" } } },
          "scale": { "$type": "number", "base": { "$value": 4 } },
          "color": {
            "$type": "color",
            "gap": { "$value": "{size.sm}" },
            "ratio": { "$ref": "#/scale/base/$value" }
          }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    // The alias target's type wins over the group type.
    assert_eq!(tokens.get("color.gap").unwrap().token_type.as_deref(), Some("dimension"));
    assert_eq!(tokens.get("color.ratio").unwrap().token_type.as_deref(), Some("number"));

    // An own type must agree with the target's type.
    let mismatch = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "weights": { "regular": { "$type": "number", "$value": 400 } },
          "heading": { "$type": "fontWeight", "$value": "{weights.regular}" }
        }"##,
    )?;
    let error = mismatch.resolve(&[]).unwrap_err();
    assert_eq!(
        error.to_string(),
        "token `heading` in `tokens.json`: reference type mismatch: expected fontWeight, found number"
    );
    Ok(())
}
```

Types follow chains: an untyped alias of an untyped alias takes the type the end of the chain
resolves to.

## JSON Pointer references

A JSON object with a `$ref` member is a reference. Its `$ref` must be a local JSON Pointer
(RFC 6901) in URI-fragment form, starting with `#/`. Pointer references may appear at any depth of
a value; their targets are not position-checked, because the resolved value as a whole is
validated against the token's type.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "base": {
            "opacity": { "$type": "number", "$value": 0.25 },
            "color": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [0.25, 0.5, 0.75] } }
          },

          "faded": { "$value": { "$ref": "#/base/opacity/$value" } },

          "tinted": {
            "$type": "color",
            "$value": {
              "colorSpace": "srgb",
              "components": [
                { "$ref": "#/base/color/$value/components/0" },
                { "$ref": "#/base/color/$value/components/1" },
                1
              ]
            }
          },

          "kind": { "$type": "fontFamily", "$value": { "$ref": "#/base/color/$type" } }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    // Value-level reference to `…/$value`: the value and the type come from the target.
    let faded = tokens.get("faded").unwrap();
    assert_eq!(faded.value, 0.25);
    assert_eq!(faded.token_type.as_deref(), Some("number"));

    // Pointers into another token's value, where a curly-brace alias is not allowed.
    assert_eq!(tokens.get("tinted").unwrap().value["components"], serde_json::json!([0.25, 0.5, 1]));

    // A `…/$type` pointer yields the target's type name as a string.
    assert_eq!(tokens.get("kind").unwrap().value, "color");
    Ok(())
}
```

There is **no implicit `/$value` suffix**. `{ "$ref": "#/base/color" }` points at the token
*object* (`$type`, `$value` and all) and, for a color token, fails validation. Point at
`#/base/color/$value` instead.

## Pointer syntax

One RFC 6901 parser handles every pointer: value-level `$ref`s, token-level `$ref`s and the
fragments of source references such as `library.json#/brand`.

- **Percent-decoding first.** As RFC 6901 §6 specifies for URI fragments, `%XX` sequences are
  decoded before the pointer is split, so `#/brand%20colors` addresses the member `brand colors`.
  An unencoded `#/brand colors` also works. A malformed `%` sequence is an `InvalidPointer` error.
- **Escapes.** In each segment, `~1` stands for `/` and `~0` for `~`. Any other `~` is an error.
- **Array indexes.** An index is `0` or a decimal number without a leading zero or sign: `+1`,
  `01` and `-1` are rejected, and the element must exist. The RFC's `-` (past-the-end) index never
  names an element.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "$extensions": { "org.example": { "brand colors": [10, 20], "grid/unit": 8 } },
          "second": { "$type": "number", "$value": { "$ref": "#/$extensions/org.example/brand%20colors/1" } },
          "grid": { "$type": "number", "$value": { "$ref": "#/$extensions/org.example/grid~1unit" } }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();
    assert_eq!(tokens.get("second").unwrap().value, 20);
    assert_eq!(tokens.get("grid").unwrap().value, 8);

    let leading_zero = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "$extensions": { "list": [1, 2] },
          "x": { "$type": "number", "$value": { "$ref": "#/$extensions/list/01" } }
        }"##,
    )?;
    assert!(matches!(leading_zero.resolve(&[]), Err(DtcgError::InvalidPointer(_))));
    Ok(())
}
```

## Token-level `$ref`

A token can be a reference itself: an object with `$ref` instead of `$value`. This is a whole-token
alias, written as a pointer.

```json
{
  "spacing": { "base": { "$type": "number", "$value": 4 } },
  "gap": { "$ref": "#/spacing/base/$value" }
}
```

- The `$ref` must be a string starting with `#/`. Otherwise the error is
  ``token-level `$ref` must be a string`` or ``token-level `$ref` must be a local JSON Pointer``.
  A token must not have both `$value` and `$ref`.
- Point at the target's `…/$value`, as the specification's examples and the official test suite
  do. A pointer to the token object itself (`#/spacing/base`) reads the whole object, which is not
  a valid value of any type, so resolution fails.
- The token's type follows the [usual order](#types-through-references): its own `$type`, else the
  target token's type when the pointer ends in `/$value`, else the group `$type`. A token-level
  `$ref` to a location that is not a token (for example under `$extensions`) therefore inherits
  its group's type.
- `ResolvedToken::authored_value` is the whole authored token object, for example
  `{"$ref": "#/spacing/base/$value"}`.
- Metadata such as `$type`, `$description`, `$deprecated` and `$extensions` is allowed beside
  `$ref`; child tokens and groups are not.

## References to arbitrary locations

A pointer does not have to target a token. It can read any JSON in the merged document, for
example a value kept under `$extensions`. Such a token takes its type from its own `$type` or its
group, because there is no target token to take one from.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "$extensions": { "org.example": { "grid/unit~v2": 8, "columns": 12 } },
          "grid": { "$type": "number", "$ref": "#/$extensions/org.example/grid~1unit~0v2" },
          "layout": {
            "$type": "number",
            "columns": { "$ref": "#/$extensions/org.example/columns" }
          }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();
    assert_eq!(tokens.get("grid").unwrap().value, 8);

    // No target token, so the group type applies.
    let columns = tokens.get("layout.columns").unwrap();
    assert_eq!(columns.value, 12);
    assert_eq!(columns.token_type.as_deref(), Some("number"));
    Ok(())
}
```

The JSON found at the pointer is itself resolved, so it may contain further aliases and references.

## Adjusting a referenced value

A reference object may carry members besides `$ref`. The referenced value must then be an object,
and each extra member is resolved and set on a copy of it. The copy replaces the reference. This is
a shallow merge: a member replaces the whole member of the same name.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{
          "brand": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [0.2, 0.4, 0.8] } },
          "brandMuted": { "$type": "color", "$value": { "$ref": "#/brand/$value", "alpha": 0.5 } }
        }"##,
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    let muted = &tokens.get("brandMuted").unwrap().value;
    assert_eq!(muted["components"], serde_json::json!([0.2, 0.4, 0.8]));
    assert_eq!(muted["alpha"], 0.5);
    Ok(())
}
```

The extra members sit at the same positions as in a literal value, so the
[alias position rules](#where-curly-brace-aliases-are-allowed) apply to them: `"alpha": "{x}"`
would be rejected inside a color.

## How a pointer is interpreted

For each `$ref` pointer, the runtime splits it into segments and finds the first segment that is
`$value` or `$type`:

- If that segment is preceded by at least one name, and those names form the path of a token in
  the current token set (`#/base/color/$value` gives `base.color`), it is a **token reference**.
  The token is resolved from the final token set, exactly like a curly-brace alias, and any further
  segments index into its resolved value.
- Otherwise it is a **document reference**. The pointer is evaluated against the **merged
  document**: the single structure all active sources fold into, where later sources win (see
  [Token documents](03-token-documents.md#several-sources-one-merged-document)). For a source
  given as `file.json#/fragment`, the fragment is that source's root. The JSON found there is then
  resolved recursively.

A pointer such as `#/$extensions/palette/$value` whose prefix is not a token is therefore a
document reference, not a missing alias. Because every pointer reads the merged document,
`#/$extensions/x` sees the value from the latest source that defines it, wherever the pointer is
written.

## Aliases see the final token set

Aliases and references are resolved **after** every source of the selection has been merged. An
alias therefore always sees the definition that won, even when the alias was written in an earlier
file than the override:

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "theme.resolver.json",
        r##"{
          "version": "2025.10",
          "sets": {
            "base": { "sources": [{
              "palette": { "$type": "color",
                "primary": { "$value": { "colorSpace": "srgb", "components": [1, 0, 0] } } },
              "button": { "background": { "$value": "{palette.primary}" } }
            }] }
          },
          "modifiers": {
            "theme": { "default": "light", "contexts": {
              "light": [],
              "dark": [{ "palette": {
                "primary": { "$value": { "colorSpace": "srgb", "components": [0, 0, 1] } } } }]
            } }
          },
          "resolutionOrder": [{ "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }]
        }"##,
    )?;

    let light = runtime.resolve(&[])?.ready().unwrap();
    let dark = runtime.resolve(&[("theme", "dark")])?.ready().unwrap();

    // `button.background` is only defined in the base set, but follows the override.
    assert_eq!(light.get("button.background").unwrap().value["components"][0], 1);
    assert_eq!(dark.get("button.background").unwrap().value["components"][2], 1);
    // The override inherits `color` from the `palette` group of the base set.
    assert_eq!(dark.get("palette.primary").unwrap().token_type.as_deref(), Some("color"));
    Ok(())
}
```

This is why a theme only needs to override the handful of palette tokens that differ: every
semantic token that aliases them follows along.

## `authored_value` and `value`

| Field | Contains |
|---|---|
| `ResolvedToken::authored_value` | The `$value` as written, with aliases and references intact. For a token-level `$ref`, the whole token object. |
| `ResolvedToken::value` | The fully resolved value: no aliases, no `$ref` objects. This is what was validated. |

Editors use `authored_value` to show and preserve aliases. Renderers use `value`.

## Cycles, depth and errors

The runtime detects reference cycles and bounds every chain of references:

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() {
    let runtime = DtcgRuntime::from_json(
        "tokens.json",
        r##"{ "a": { "$ref": "#/b/$value" }, "b": { "$ref": "#/a/$value" } }"##,
    )
    .unwrap();

    match runtime.resolve(&[]) {
        Err(DtcgError::Token { path, message, .. }) => {
            assert_eq!(path, "a");
            assert_eq!(message, "alias cycle: a -> b -> a");
        }
        other => panic!("expected a cycle, got {other:?}"),
    }
}
```

| Situation | Error |
|---|---|
| Alias to a path with no token, including a group path and `{}` | `UnresolvedReference`: ``unresolved reference `<path>`: missing alias target`` |
| An alias at a position that does not admit one | `Token`: ``curly-brace references are not allowed inside <type> values`` |
| An alias at a composite position whose target has the wrong type | `TokenType`: ``reference type mismatch: expected <type>, found <type>`` |
| An own `$type` that differs from a whole-value reference's target type | `TokenType`: ``reference type mismatch: expected <own>, found <target>`` |
| Tokens that alias each other in a loop | `Token`: `alias cycle: a -> b -> a` |
| Document pointers that reference each other in a loop | `InvalidPointer`: ``circular JSON Pointer reference `#/…` `` |
| A pointer to a location that does not exist, including a pointer to a missing token | `InvalidPointer` with the pointer |
| A pointer that does not start with `#/`, a bad `~` or `%` escape, or an invalid array index | `InvalidPointer` |
| A reference with extra members whose target is not an object | `InvalidPointer` with the pointer |
| A chain of more than `RuntimeLimits::max_alias_depth` references (default 64) | `LimitExceeded { kind: "alias depth", .. }` |
| References that copy more than `RuntimeLimits::max_value_nodes` JSON nodes (default 4,000,000) | `LimitExceeded { kind: "value nodes", .. }` |
| A resolved value nested more than 128 levels deep | `LimitExceeded { kind: "value depth", .. }` |

`max_alias_depth` counts the reference hops along one chain: curly-brace aliases, token pointers
and document pointers alike. The count does not depend on the order in which tokens are authored or
resolved, and literal values cost nothing, so a limit of 0 still resolves tokens without
references. `max_value_nodes` stops pointer fan-out (references to references that duplicate a
value at every level) before it builds an exponentially large value. See
[Errors, limits and diagnostics](11-errors-limits-and-diagnostics.md#runtime-limits).
