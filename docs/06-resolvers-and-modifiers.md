# 6. Resolvers and modifiers

A resolver document says which token sources make up a design system, in what order to stack them,
and which variants can be switched at run time. This chapter covers the resolver format as
`dtcg_runtime` implements it, how sources are found and stacked, and how selections pick
contexts.

- [Resolver or token document?](#resolver-or-token-document)
- [Resolver structure](#resolver-structure)
- [Sets](#sets)
- [Modifiers and contexts](#modifiers-and-contexts)
- [`resolutionOrder`](#resolutionorder)
- [Sources](#sources)
- [Nested resolvers](#nested-resolvers)
- [Overlay semantics](#overlay-semantics)
- [Selections](#selections)
- [Discovering modifiers for a UI](#discovering-modifiers-for-a-ui)
- [Validating every context](#validating-every-context)
- [Errors](#errors)

## Resolver or token document?

Any JSON object that has a `resolutionOrder` member is treated as a resolver. Everything else is a
token document. That applies to the root and to every source:

| Root | Behavior |
|---|---|
| A token document | It is the only source. `modifiers()` returns no axes, and selections are ignored. |
| A resolver | Its `resolutionOrder` decides which sources are stacked, and selections pick modifier contexts. |

A source inside a resolver can itself be a resolver, inline or in another file. See
[nested resolvers](#nested-resolvers).

## Resolver structure

```json
{
  "version": "2025.10",
  "name": "Acme",
  "description": "Acme design system",
  "sets": {
    "foundation": { "sources": [{ "$ref": "tokens/foundation.json" }] },
    "semantic": { "sources": [{ "$ref": "tokens/semantic.json" }] }
  },
  "modifiers": {
    "theme": {
      "default": "light",
      "contexts": {
        "light": [],
        "dark": [{ "$ref": "tokens/dark.json" }]
      }
    }
  },
  "resolutionOrder": [
    { "$ref": "#/sets/foundation" },
    { "$ref": "#/sets/semantic" },
    { "$ref": "#/modifiers/theme" }
  ]
}
```

| Member | Required | Rule |
|---|---|---|
| `version` | yes | Exactly `"2025.10"`. Other versions, or none, fail with ``resolver version must be `2025.10` ``. |
| `resolutionOrder` | yes | An array of steps. |
| `sets` | no | An object of named set definitions. |
| `modifiers` | no | An object of named modifier definitions. |
| anything else | no | Ignored, but reachable through local pointer sources such as `{ "$ref": "#/$defs/motion" }`. `name`, `description`, `$schema`, `$extensions` and `$defs` are all fine. |

A resolver is checked when discovery reaches it, which means on every `resolve` that involves it.

## Sets

```json
{ "sources": [ { "$ref": "tokens/foundation.json" }, { "radius": { "$type": "number", "$value": 4 } } ] }
```

A set is an ordered list of [sources](#sources) that is always applied where it appears. `sources`
is required and must be an array; it may be empty. A set that nothing references from
`resolutionOrder` or from another source is *dormant*: it is never applied, and its documents are
never requested by `resolve`. The [inspection APIs](10-source-inspection.md) do still report it.

## Modifiers and contexts

```json
{
  "default": "light",
  "contexts": {
    "light": [],
    "dark": [{ "$ref": "tokens/dark.json" }],
    "high-contrast": [{ "$ref": "tokens/dark.json" }, { "$ref": "tokens/hc.json" }]
  }
}
```

- `contexts` is required: a non-empty object whose members are arrays of sources. Each array may be
  empty, which is how a context says "nothing changes".
- `default` is optional. If present, it must name one of the contexts. A modifier without a default
  must be selected explicitly on every `resolve`.
- Only the selected context's sources are applied, and only its documents are requested. The other
  contexts are inactive.
- Context order is the authored order. `modifiers()` reports contexts in that order.

> A modifier must have a selected or default context even if `resolutionOrder` never references
> it. Declared modifiers are validated together.

## `resolutionOrder`

`resolutionOrder` is an array of steps, applied first to last. A step is one of:

| Step | Example |
|---|---|
| A reference to a set | `{ "$ref": "#/sets/foundation" }` |
| A reference to a modifier | `{ "$ref": "#/modifiers/theme" }` |
| An inline set | `{ "type": "set", "name": "overrides", "sources": [ … ] }` |
| An inline modifier | `{ "type": "modifier", "name": "density", "default": "comfortable", "contexts": { … } }` |

Rules:

- Each step is an object. A `$ref` step must point to `#/sets/<name>` or `#/modifiers/<name>`, and
  the definition must exist. Names in pointers use JSON Pointer escaping (`~1` for `/`, `~0` for
  `~`).
- An inline step needs `type` (`"set"` or `"modifier"`) and a non-empty `name`. Inline names must
  be unique within one `resolutionOrder`.
- An inline modifier is a modifier like any other: it appears in `modifiers()` and is selected by
  its `name`.
- The same set may be referenced by more than one step. It is then applied at each position.

## Sources

Sources appear in set `sources` arrays and in modifier context arrays. Every source is a JSON
object of one of these kinds:

| Kind | Example | Meaning |
|---|---|---|
| Inline token document | `{ "radius": { "$type": "dimension", "$value": … } }` | Tokens written inside the resolver |
| External document | `{ "$ref": "tokens/base.json" }` | Another document, resolved relative to the referencing document |
| External fragment | `{ "$ref": "library.json#/palette" }` | Part of another document. The subtree is the source root, so token paths are relative to it. |
| Local set | `{ "$ref": "#/sets/foundation" }` | The sources of that set, spliced in at this position |
| Local JSON | `{ "$ref": "#/$defs/motion" }` | Any other location in the same resolver |
| Reference with overrides | `{ "$ref": "library.json#/spacing", "lg": { … } }` | The target, with its top-level members replaced by the extra members |
| Nested resolver | `{ "$ref": "brand/brand.resolver.json" }`, or an inline object with `resolutionOrder` | Expanded in place; see [nested resolvers](#nested-resolvers) |

Local references into `#/resolutionOrder/…` or `#/modifiers/…` are not allowed as sources, and
local references must not form a cycle.

This example uses every kind except nested resolvers:

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

const RESOLVER: &str = r##"{
  "version": "2025.10",
  "$defs": {
    "motion": { "fast": { "$type": "duration", "$value": { "value": 120, "unit": "ms" } } }
  },
  "sets": {
    "core": {
      "sources": [
        { "$ref": "core/palette.json" },
        { "$ref": "core/library.json#/spacing" },
        { "$ref": "#/$defs/motion" },
        { "radius": { "$type": "dimension", "$value": { "value": 4, "unit": "px" } } }
      ]
    },
    "product": {
      "sources": [
        { "$ref": "#/sets/core" },
        {
          "$ref": "core/library.json#/spacing",
          "lg": { "$type": "dimension", "$value": { "value": 32, "unit": "px" } }
        }
      ]
    }
  },
  "resolutionOrder": [{ "$ref": "#/sets/product" }]
}"##;

const PALETTE: &str = r##"{
  "ink": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [0.1, 0.1, 0.1] } }
}"##;

const LIBRARY: &str = r##"{
  "spacing": {
    "$type": "dimension",
    "sm": { "$value": { "value": 4, "unit": "px" } },
    "lg": { "$value": { "value": 16, "unit": "px" } }
  },
  "unused": { "$type": "number", "$value": 0 }
}"##;

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json("theme.resolver.json", RESOLVER)?;
    runtime.provide_json_batch([("core/palette.json", PALETTE), ("core/library.json", LIBRARY)])?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    assert_eq!(tokens.get("ink").unwrap().source.as_str(), "core/palette.json");
    assert_eq!(tokens.get("fast").unwrap().value["value"], 120); // from #/$defs/motion
    assert_eq!(tokens.get("radius").unwrap().value["value"], 4); // inline

    // The fragment's tokens are relative to `#/spacing`, and `unused` was never included.
    assert_eq!(tokens.get("sm").unwrap().value["value"], 4);
    assert!(tokens.get("spacing.sm").is_none() && tokens.get("unused").is_none());

    // The reference object in `product` replaced the fragment's `lg` member.
    assert_eq!(tokens.get("lg").unwrap().value["value"], 32);
    assert_eq!(tokens.get("lg").unwrap().source.as_str(), "core/library.json");
    Ok(())
}
```

### Reference objects with overrides

When a source reference has members besides `$ref`, the runtime loads the target (which must be a
JSON object) and replaces its **top-level** members with the extra members, one whole member at a
time. In the example above, `"lg": {…}` replaced the target's `lg` member. If the target were the
whole library document, an extra `"spacing": {…}` member would replace the entire `spacing` group,
not merge into it.

The resulting tokens report the target document as their `source`. The
[inspection APIs](10-source-inspection.md) record which members came from the reference object.

### Relative references

External references resolve against the **document that contains them**, like relative URLs:
`tokens/dark.json` written in `design/acme.resolver.json` becomes `design/tokens/dark.json`. A
reference inside a nested resolver resolves against that nested resolver. The rules for resource
ids, including `..`, are in [Document loading](07-document-loading.md#resource-ids).

## Nested resolvers

A source whose JSON contains `resolutionOrder` is a resolver. It is expanded at its position: its
own `resolutionOrder` is walked with its own sets and modifiers, and the terminal sources it
produces take the place of the reference. This lets one design system build on another.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

const PRODUCT: &str = r##"{
  "version": "2025.10",
  "sets": { "brand": { "sources": [{ "$ref": "brand/brand.resolver.json" }] } },
  "modifiers": { "theme": { "default": "light", "contexts": {
    "light": [],
    "dark": [{ "surface": { "$type": "number", "$value": 10 } }]
  } } },
  "resolutionOrder": [{ "$ref": "#/sets/brand" }, { "$ref": "#/modifiers/theme" }]
}"##;

// The nested resolver declares the same `theme` modifier, so it follows the selection.
const BRAND: &str = r##"{
  "version": "2025.10",
  "sets": { "base": { "sources": [{ "$ref": "base.json" }] } },
  "modifiers": { "theme": { "default": "light", "contexts": {
    "light": [],
    "dark": [{ "accent": { "$type": "number", "$value": 2 } }]
  } } },
  "resolutionOrder": [{ "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }]
}"##;

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json("product.resolver.json", PRODUCT)?;
    runtime.provide_json("brand/brand.resolver.json", BRAND)?;
    // `base.json` inside the brand resolver resolves to `brand/base.json`.
    runtime.provide_json(
        "brand/base.json",
        r#"{ "accent": { "$type": "number", "$value": 1 }, "surface": { "$type": "number", "$value": 0 } }"#,
    )?;

    let dark = runtime.resolve(&[("theme", "dark")])?.ready().unwrap();
    assert_eq!(dark.get("accent").unwrap().value, 2); // the brand's own dark context
    assert_eq!(dark.get("surface").unwrap().value, 10); // the product's dark context
    Ok(())
}
```

Rules:

- **Selections apply to every resolver in the graph.** The same `&[(modifier, context)]` is used
  for the root and for each nested resolver. Every modifier you select explicitly must be declared
  by **each** resolver that discovery passes through. Otherwise resolution fails with
  ``unknown input `theme` `` naming the nested resolver. A nested resolver's modifiers that are not
  selected use their defaults. Either share the modifier vocabulary across nested resolvers, or
  reference the nested design system's token files directly instead of its resolver.
- **Cycles.** A resolver that, directly or indirectly, includes itself fails with
  `DtcgError::DocumentCycle`. Only the chain of resolvers being expanded counts, so including the
  same resolver at two different positions is fine; it is expanded at both.
- **Limits.** The nesting depth is bounded by `RuntimeLimits::max_reference_depth` (default 64).

## Overlay semantics

After discovery, the runtime has an ordered list of terminal token sources. It flattens each into
token paths and stacks them:

1. Sources are processed in discovery order: `resolutionOrder` steps first to last, each set's or
   context's sources in array order, with references and nested resolvers expanded in place.
2. A token whose path was already defined **replaces the earlier token entirely**: value, type,
   `$extensions` and `source`.
3. The replaced token moves to the position of its last definition in
   [token order](03-token-documents.md#token-order).
4. Groups are not merged as JSON objects; only tokens are stacked. A later source can add one token
   to an existing group without repeating the others.
5. After stacking, aliases, types and validation run on the final set (see
   [Aliases see the final token set](04-aliases-and-references.md#aliases-see-the-final-token-set)).

Three consequences matter when writing override files:

- **Composite values are replaced, not merged.** To change only the width of a border, the
  override must still give the full `{ color, width, style }` value. It cannot borrow members from
  the token it replaces (that would be an alias to itself), but it can alias the palette and size
  tokens the original was built from.
- **An override does not inherit the replaced token's type.** Repeat the `$type` on the token or
  on its group in the override file, or make the override a whole-value alias of a typed token.
  Otherwise it fails with `token requires a DTCG type`.
- **Tokens cannot be removed.** Later sources can add and replace, but not delete.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime};

fn resolver(override_source: &str) -> String {
    format!(
        r##"{{
          "version": "2025.10",
          "resolutionOrder": [
            {{ "type": "set", "name": "base", "sources": [
              {{ "size": {{ "$type": "number", "sm": {{ "$value": 4 }}, "lg": {{ "$value": 16 }} }} }}
            ] }},
            {{ "type": "set", "name": "compact", "sources": [{override_source}] }}
          ]
        }}"##
    )
}

fn main() -> Result<(), DtcgError> {
    // The override has no type of its own, and does not inherit `number` from the base file.
    let untyped = DtcgRuntime::from_json("r.json", resolver(r#"{ "size": { "lg": { "$value": 12 } } }"#))?;
    assert!(matches!(untyped.resolve(&[]), Err(DtcgError::TokenType { .. })));

    // Repeating the group type in the override file fixes it.
    let typed = DtcgRuntime::from_json(
        "r.json",
        resolver(r#"{ "size": { "$type": "number", "lg": { "$value": 12 } } }"#),
    )?;
    let tokens = typed.resolve(&[])?.ready().unwrap();
    assert_eq!(tokens.get("size.lg").unwrap().value, 12);
    assert_eq!(tokens.get("size.sm").unwrap().value, 4); // untouched tokens remain
    Ok(())
}
```

## Selections

A selection is a slice of `(modifier, context)` pairs:

```rust
use dtcg_runtime::DtcgRuntime;

fn example(runtime: &DtcgRuntime) {
    let _ = runtime.resolve(&[]); // every modifier at its default
    let _ = runtime.resolve(&[("theme", "dark")]);
    let _ = runtime.resolve(&[("theme", "dark"), ("density", "compact")]); // order does not matter
}
```

- Modifiers left out use their `default`. A modifier without a default must be selected.
- Naming a modifier the resolver does not declare, a context the modifier does not have, or the
  same modifier twice is an error.
- For a root that is a plain token document, the selection is ignored.

`resolve` reports selection problems as `InvalidDocument` or `Token` errors against the resolver
document. For a user-facing picker, check the selection with
[`normalize_selection`](#discovering-modifiers-for-a-ui) first, which returns `UnknownModifier` and
`UnknownContext` errors that list the allowed values.

| Selection problem | Error from `resolve` | Error from `normalize_selection` |
|---|---|---|
| Unknown modifier | ``invalid DTCG document `r.json`: unknown input `size` `` | ``unknown modifier `size`; known modifiers: theme, density`` |
| Unknown context | ``token `#/modifiers/theme` in `r.json`: selected modifier context is unknown`` | ``modifier `theme` context `blue`: selected modifier context is unknown; allowed contexts: light, dark`` |
| No default, not selected | ``invalid DTCG document `r.json`: missing input for modifier `density` `` | ```modifier `density` context ``: modifier has no default and was not selected; allowed contexts: comfortable, compact``` |
| Selected twice | ``invalid DTCG document `r.json`: duplicate input `theme` `` | ``modifier `theme` context `dark`: modifier is selected twice; allowed contexts: light, dark`` |

## Discovering modifiers for a UI

`DtcgRuntime::modifiers()` describes the root resolver's inputs as `ModifierAxis` values:

```rust,ignore
pub struct ModifierAxis {
    pub name: String,            // the modifier name to use in a selection
    pub contexts: Vec<String>,   // its contexts, in authored order
    pub default: Option<String>, // its default context, if any
}
```

Axes are listed in `resolutionOrder` order (referenced and inline modifiers alike), followed by the
root modifiers that `resolutionOrder` does not mention, in authored order. Only the **root**
resolver is described; nested resolvers' modifiers are not listed.

`normalize_selection` completes a selection with defaults, validates it, and returns it in axis
order. Two selections that mean the same thing normalize to equal vectors, which makes the result a
good cache key for resolved tokens.

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, Resolution};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "ui.resolver.json",
        r##"{
          "version": "2025.10",
          "modifiers": {
            "theme": { "default": "light", "contexts": { "light": [], "dark": [] } }
          },
          "resolutionOrder": [
            { "type": "modifier", "name": "density", "default": "comfortable",
              "contexts": { "comfortable": [], "compact": [] } },
            { "$ref": "#/modifiers/theme" }
          ]
        }"##,
    )?;

    let Resolution::Ready(axes) = runtime.modifiers()? else { unreachable!() };
    let names: Vec<&str> = axes.iter().map(|axis| axis.name.as_str()).collect();
    assert_eq!(names, ["density", "theme"]);

    let a = runtime.normalize_selection(&[("theme", "dark")])?.ready().unwrap();
    let b = runtime.normalize_selection(&[("theme", "dark"), ("density", "comfortable")])?.ready().unwrap();
    assert_eq!(a, b);

    let error = runtime.normalize_selection(&[("theme", "sepia")]).unwrap_err();
    assert!(matches!(error, DtcgError::UnknownContext { ref allowed, .. } if allowed == &["light", "dark"]));
    Ok(())
}
```

Both methods return `Resolution::Missing` with a request for the root document if it has not been
provided yet.

## Validating every context

`resolve` only looks at the selected contexts. Two tools cover the rest.

**`validate_complete()`** walks every context of every modifier, as if all were active. It returns
`Missing` with every document any context needs, and `Ready(())` once all are present and the
resolver structure is valid. It does **not** resolve or validate token values. Use it to preload
everything, for example before going offline or bundling assets.

**Resolving every combination** checks the token values too. With a few modifiers, the cartesian
product of their contexts is small:

```rust
use dtcg_runtime::{DtcgError, DtcgRuntime, ModifierAxis};

fn all_selections(axes: &[ModifierAxis]) -> Vec<Vec<(&str, &str)>> {
    let mut selections = vec![Vec::new()];
    for axis in axes {
        selections = selections
            .into_iter()
            .flat_map(|selection| {
                axis.contexts.iter().map(move |context| {
                    let mut next = selection.clone();
                    next.push((axis.name.as_str(), context.as_str()));
                    next
                })
            })
            .collect();
    }
    selections
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "r.json",
        r##"{
          "version": "2025.10",
          "sets": { "base": { "sources": [{ "gap": { "$type": "number", "$value": 8 } }] } },
          "modifiers": {
            "theme": { "default": "light", "contexts": {
              "light": [],
              "dark": [{ "gap": { "$type": "number", "$value": "{missing}" } }]
            } },
            "density": { "default": "comfortable", "contexts": { "comfortable": [], "compact": [] } }
          },
          "resolutionOrder": [
            { "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }, { "$ref": "#/modifiers/density" }
          ]
        }"##,
    )?;

    // Structure and documents are fine in every context...
    assert!(runtime.validate_complete()?.ready().is_some());

    // ...but the dark context has a dangling alias.
    let axes = runtime.modifiers()?.ready().unwrap();
    let failing: Vec<_> = all_selections(&axes)
        .into_iter()
        .filter(|selection| runtime.resolve(selection).is_err())
        .collect();
    assert_eq!(
        failing,
        [
            vec![("theme", "dark"), ("density", "comfortable")],
            vec![("theme", "dark"), ("density", "compact")]
        ]
    );
    Ok(())
}
```

In a CI job for a token repository, run this over every combination and fail on any error.

## Errors

| Situation | Error |
|---|---|
| `version` missing or not `2025.10` | `InvalidDocument`: ``resolver version must be `2025.10` `` |
| `resolutionOrder` missing or not an array | `InvalidDocument`: `resolutionOrder is required` |
| `sets` or `modifiers` not an object | `InvalidDocument`: `resolver sets must be an object` / `resolver modifiers must be an object` |
| A step that is not an object | `InvalidDocument`: `resolution step must be an object` |
| A `$ref` step that is not `#/sets/…` or `#/modifiers/…` | `InvalidDocument`: ``unknown resolver source `…` `` |
| A `$ref` step to a set or modifier that is not defined | `InvalidPointer` |
| An inline step without `type` or `name`, or another `type` | `InvalidDocument`: `inline resolution step requires …` / `` resolution step type must be `set` or `modifier` `` |
| Two inline steps with the same name | `InvalidDocument`: ``duplicate inline resolution name `…` `` |
| A set without a `sources` array | `InvalidDocument`: `set sources are required` |
| A modifier without contexts, or a context that is not an array | `InvalidDocument`: `modifier requires contexts` / `modifier context must be an array` |
| A `default` that is not a string or not a context | `InvalidDocument`: `modifier default must be a string` / `modifier default context is unknown` |
| A source that is not an object | `InvalidDocument`: `document source must be an object` |
| A source reference to `#/resolutionOrder/…` or `#/modifiers/…` | `InvalidDocument`: ``forbidden source reference `…` `` |
| A source reference to an unknown set | `InvalidDocument`: ``unknown set `…` `` |
| Local source references that form a cycle | `InvalidDocument`: ``reference cycle at `…` `` |
| An override on a reference whose target is not an object | `InvalidDocument`: `reference target must be an object` |
| A fragment pointer that does not exist in the target | `InvalidPointer` |
| A resolver that includes itself | `DocumentCycle` |
| A malformed or escaping reference path | `InvalidResourceId` / `InvalidPointer` |
| Selection problems | See [Selections](#selections) |
