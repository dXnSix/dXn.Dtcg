---
title: Resolvers and modifiers
description: Resolver documents, sets, modifiers, sources, nested resolvers, overlay semantics, selections and how to validate every context.
group: Concepts
order: 80
---

A resolver document says which token sources make up a design system, in what order they merge, and which variants can be switched at run time. This page covers the resolver format as `dxn_dtcg` implements it (DTCG Resolver 2025.10).

## Resolver or token document?

Any JSON object with a `resolutionOrder` member is a resolver; everything else is a token document, even one with a top-level group named `modifiers` or `sets`. That applies to the root and to every source.

| Root | Behavior |
|---|---|
| A token document | It is the only source. `modifiers()` returns no axes and selections are ignored |
| A resolver | `resolutionOrder` decides which sources merge, and selections pick modifier contexts |

## Structure

```json
{
  "$schema": "https://www.designtokens.org/schemas/2025.10/resolver.json",
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

Only the members Resolver section 4 defines are allowed; anything else fails with ``unknown resolver member `<name>` in <location>``.

| Location | Allowed members |
|---|---|
| The root | `version` (required, exactly `"2025.10"`), `resolutionOrder` (required, non-empty), `name`, `description`, `sets`, `modifiers`, `$schema`, `$defs` |
| A set | `sources` (required), `description`, `$extensions` |
| A modifier | `contexts` (required), `default`, `description`, `$extensions` |
| An inline set step | `type`, `name`, `sources`, `description`, `$extensions` |
| An inline modifier step | `type`, `name`, `contexts`, `default`, `description`, `$extensions` |
| A `$ref` step or source reference | `$ref` plus any other members, which are overrides |

The root does not accept `$extensions` or `x-*` members. Put shared JSON under `$defs` and reach it with `{ "$ref": "#/$defs/motion" }`. The same rules apply to nested resolvers and to the `data` of a Token Editor envelope.

## Sets

A set is an ordered list of sources that is always applied where it appears. `sources` is required and may be empty. A set nothing references is *dormant*: it is never applied and `resolve` never requests its documents, though the [inspection APIs](./inspection.md) still report it.

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

- `contexts` is required and has **at least two** members, each an array of sources. An empty array says "nothing changes". A modifier with one context fails with `modifier requires at least two contexts`; use a set for always-applied sources.
- `default` is optional and must name a context. A modifier without one must be selected explicitly.
- Only the selected context's sources are applied and requested.
- Every root modifier is an input, even one `resolutionOrder` never references: `modifiers()` lists it and it needs a default or a selection.

## resolutionOrder

A non-empty array of steps, applied first to last:

| Step | Example |
|---|---|
| Reference to a set | `{ "$ref": "#/sets/foundation" }` |
| Reference to a modifier | `{ "$ref": "#/modifiers/theme" }` |
| Reference with overrides | `{ "$ref": "#/modifiers/theme", "default": "dark" }` |
| Inline set | `{ "type": "set", "name": "overrides", "sources": [ ... ] }` |
| Inline modifier | `{ "type": "modifier", "name": "density", "default": "comfortable", "contexts": { ... } }` |

A `$ref` step must point to `#/sets/<name>` or `#/modifiers/<name>`, and the definition must exist. Members next to `$ref` replace the definition's top-level members, and the result is the one effective definition every API uses. Inline steps need `type` and a non-empty, unique `name`. A modifier may be defined by only one step (``resolutionOrder defines modifier `<name>` more than once``). The same set may appear in more than one step and is applied at each position.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "r.json",
        r##"{
          "version": "2025.10",
          "modifiers": { "theme": { "default": "light", "contexts": {
            "light": [{ "bg": { "$type": "number", "$value": 0 } }],
            "dark": [{ "bg": { "$type": "number", "$value": 1 } }]
          } } },
          "resolutionOrder": [{ "$ref": "#/modifiers/theme", "default": "dark" }]
        }"##,
    )?;

    // The step's `default` override is the default everywhere.
    let axes = runtime.modifiers()?.ready().unwrap();
    assert_eq!(axes[0].default.as_deref(), Some("dark"));
    assert_eq!(runtime.resolve(&[])?.ready().unwrap().get("bg").unwrap().value, 1);
    assert_eq!(
        runtime.normalize_selection(&[])?.ready().unwrap(),
        [("theme".to_owned(), "dark".to_owned())]
    );
    Ok(())
}
```

## Sources

Sources appear in set `sources` arrays and modifier contexts. Each is a JSON object of one kind:

| Kind | Example | Meaning |
|---|---|---|
| Inline token document | `{ "radius": { "$type": "dimension", "$value": ... } }` | Tokens written inside the resolver |
| External document | `{ "$ref": "tokens/base.json" }` | Another document, relative to the referencing one |
| External fragment | `{ "$ref": "library.json#/palette" }` | Part of a document; the subtree is the source root, so paths are relative to it |
| Local set | `{ "$ref": "#/sets/foundation" }` | That set's sources, spliced in here |
| Local JSON | `{ "$ref": "#/$defs/motion" }` | Any other location in the same resolver |
| Reference with overrides | `{ "$ref": "library.json#/spacing", "lg": { ... } }` | The target, with top-level members replaced by the extra members |
| Nested resolver | `{ "$ref": "brand/brand.resolver.json" }` or an inline object with `resolutionOrder` | Expanded in place |

Local references into `#/resolutionOrder/...` or `#/modifiers/...` are not allowed as sources, and local references must not form a cycle. This example uses every kind but nested resolvers:

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

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

    // The fragment's tokens are relative to `#/spacing`, and `unused` was never included.
    assert_eq!(tokens.get("sm").unwrap().value["value"], 4);
    assert!(tokens.get("spacing.sm").is_none() && tokens.get("unused").is_none());

    // The reference object in `product` replaced the fragment's `lg` member.
    assert_eq!(tokens.get("lg").unwrap().value["value"], 32);
    Ok(())
}
```

External references resolve against the **document that contains them**, like relative URLs: `tokens/dark.json` written in `design/acme.resolver.json` becomes `design/tokens/dark.json`. A reference inside a nested resolver resolves against that nested resolver. See [resource ids](./loading.md#resource-ids).

:::note Overrides are shallow
A reference object replaces the target's top-level members one whole member at a time. If the target is the whole library and you add `"spacing": {...}`, the entire `spacing` group is replaced, not merged. Resulting tokens report the target document as their `source`.
:::

## Nested resolvers

A source whose JSON contains `resolutionOrder` is expanded in place, with its own sets and modifiers. This lets one design system build on another. Nested resolvers are a runtime extension that Resolver 2025.10 does not define.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

const PRODUCT: &str = r##"{
  "version": "2025.10",
  "sets": { "brand": { "sources": [{ "$ref": "brand/brand.resolver.json" }] } },
  "modifiers": { "theme": { "default": "light", "contexts": {
    "light": [],
    "dark": [{ "surface": { "$type": "number", "$value": 10 } }]
  } } },
  "resolutionOrder": [{ "$ref": "#/sets/brand" }, { "$ref": "#/modifiers/theme" }]
}"##;

// The brand resolver also declares `theme`, so it follows the same input. Its
// `contrast` modifier exists only here.
const BRAND: &str = r##"{
  "version": "2025.10",
  "sets": { "base": { "sources": [{ "$ref": "base.json" }] } },
  "modifiers": {
    "theme": { "default": "light", "contexts": {
      "light": [],
      "dark": [{ "accent": { "$type": "number", "$value": 2 } }]
    } },
    "contrast": { "default": "normal", "contexts": {
      "normal": [],
      "high": [{ "accent": { "$type": "number", "$value": 3 } }]
    } }
  },
  "resolutionOrder": [
    { "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }, { "$ref": "#/modifiers/contrast" }
  ]
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

    // `contrast` is valid because a reached resolver declares it.
    let high = runtime.resolve(&[("contrast", "high")])?.ready().unwrap();
    assert_eq!(high.get("accent").unwrap().value, 3);
    Ok(())
}
```

- **One selection serves the whole graph.** An input is valid when **any** reached resolver declares it; each resolver applies the inputs naming its own modifiers and its defaults for the rest. An input no reached resolver declares is `UnknownModifier`.
- **Missing inputs are per resolver:** a resolver requires an input only for its own modifiers without a default.
- **Missing documents come first.** While a nested resolver is not loaded its modifiers are unknown, so `resolve` returns `Missing` rather than an unknown-input error.
- **Cycles** (a resolver including itself, directly or indirectly) fail with `DocumentCycle`. Including the same resolver at two positions is fine.
- **Limits:** nesting depth is bounded by `max_reference_depth` (64) and each expanded resolver counts toward `max_documents`.

## Overlay semantics

After discovery the runtime has an ordered list of terminal sources and folds them into one merged structure; the rules (groups merge, tokens replace whole, aliases resolve last) are in [Token documents](./token-documents.md#several-sources-one-merged-document). Consequences for override files:

- **An override inherits its group's type.** `{ "size": { "lg": { "$value": 12 } } }` over a file declaring `"size": { "$type": "number", ... }` is a `number` token. A group `$type` in the override replaces the inherited one for that group.
- **Composite values are replaced, not merged.** To change only a border's width, the override must give the full `{ color, width, style }`. It can alias the palette and size tokens the original used.
- **Tokens cannot be removed** by a later source.

## Selections

A selection is a slice of `(modifier, context)` pairs.

```rust
use dxn_dtcg::DtcgRuntime;

fn example(runtime: &DtcgRuntime) {
    let _ = runtime.resolve(&[]); // every modifier at its default
    let _ = runtime.resolve(&[("theme", "dark")]);
    let _ = runtime.resolve(&[("theme", "dark"), ("density", "compact")]); // order does not matter
}

fn main() {
    example(&DtcgRuntime::default());
}
```

- **Names match case-insensitively.** An exact match wins, otherwise names compare after lowercasing. A name matching several declared names case-insensitively and none exactly is an error.
- Modifiers left out use their `default`; one without a default must be selected.
- Naming an unknown modifier or context, or the same modifier twice, is an error.
- A graph without modifiers ignores the selection.

`resolve` and `normalize_selection` validate selections identically and list the allowed values, so a picker can show them:

| Problem | Error |
|---|---|
| Unknown modifier | `UnknownModifier`: ``unknown modifier `size`; known modifiers: theme, density`` (declaration order, root first) |
| Modifier name matches several | `UnknownModifier`, `known` lists the candidates |
| Unknown context | `UnknownContext`: ``modifier `theme` context `blue`: selected modifier context is unknown; allowed contexts: light, dark`` |
| Context matches several | `UnknownContext`: `context matches more than one declared context` |
| No default, not selected | `UnknownContext` with an empty `context`: ``modifier has no default and was not selected`` |
| Selected twice | `UnknownContext`: `modifier is selected twice` |

Selections that arrive as JSON, such as resolver inputs `{"theme": "dark"}` from a URL or settings file, convert with `selection_from_json`; `selection_refs` borrows the owned pairs as the `&[(&str, &str)]` every method takes. Anything but an object of strings is `DtcgError::InvalidInput`.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, selection_from_json, selection_refs};
use serde_json::json;

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "r.json",
        r##"{
          "version": "2025.10",
          "modifiers": {
            "theme": { "default": "light", "contexts": {
              "light": [{ "bg": { "$type": "number", "$value": 0 } }],
              "dark": [{ "bg": { "$type": "number", "$value": 1 } }]
            } },
            "density": { "contexts": { "comfortable": [], "compact": [] } }
          },
          "resolutionOrder": [{ "$ref": "#/modifiers/theme" }, { "$ref": "#/modifiers/density" }]
        }"##,
    )?;

    // Case-insensitive names.
    let tokens = runtime.resolve(&[("Theme", "DARK"), ("density", "compact")])?.ready().unwrap();
    assert_eq!(tokens.get("bg").unwrap().value, 1);

    // `density` has no default.
    let error = runtime.resolve(&[("theme", "dark")]).unwrap_err();
    assert!(matches!(
        &error,
        DtcgError::UnknownContext { modifier, context, message, .. }
            if modifier == "density" && context.is_empty()
                && message == "modifier has no default and was not selected"
    ));

    // An unknown modifier lists the declared ones.
    assert!(matches!(
        runtime.resolve(&[("size", "large"), ("density", "compact")]),
        Err(DtcgError::UnknownModifier { known, .. }) if known == ["theme", "density"]
    ));

    // JSON inputs.
    let inputs = selection_from_json(&json!({ "theme": "dark", "density": "compact" }))?;
    assert!(runtime.resolve(&selection_refs(&inputs))?.is_ready());
    Ok(())
}
```

## Discovering modifiers for a UI

`DtcgRuntime::modifiers()` describes the **root** resolver's inputs as `ModifierAxis { name, contexts, default }` values, in `resolutionOrder` order (referenced and inline modifiers alike, with step overrides applied), followed by root modifiers `resolutionOrder` does not mention. Nested resolvers' modifiers are not listed, and `modifiers()` loads no document other than the root.

`normalize_selection` validates a selection exactly as `resolve` does across every resolver the graph reaches, and returns the chosen context of **every reached modifier**, nested ones included, with the declared spellings. Equal selections normalize to equal vectors, which makes the result a cache key.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, Resolution};

fn main() -> Result<(), DtcgError> {
    let mut runtime = DtcgRuntime::from_json(
        "ui.resolver.json",
        r##"{
          "version": "2025.10",
          "modifiers": {
            "theme": { "default": "light", "contexts": { "light": [], "dark": [] } }
          },
          "resolutionOrder": [
            { "type": "modifier", "name": "density", "default": "comfortable",
              "contexts": { "comfortable": [], "compact": [] } },
            { "$ref": "#/modifiers/theme" },
            { "type": "set", "name": "brand", "sources": [{ "$ref": "brand.resolver.json" }] }
          ]
        }"##,
    )?;
    runtime.provide_json(
        "brand.resolver.json",
        r##"{
          "version": "2025.10",
          "modifiers": { "contrast": { "default": "normal", "contexts": { "normal": [], "high": [] } } },
          "resolutionOrder": [{ "$ref": "#/modifiers/contrast" }]
        }"##,
    )?;

    // Only the root's axes.
    let Resolution::Ready(axes) = runtime.modifiers()? else { unreachable!() };
    let names: Vec<&str> = axes.iter().map(|axis| axis.name.as_str()).collect();
    assert_eq!(names, ["density", "theme"]);

    // The whole graph, with declared spellings.
    let a = runtime.normalize_selection(&[("THEME", "Dark")])?.ready().unwrap();
    let b = runtime
        .normalize_selection(&[("theme", "dark"), ("density", "comfortable"), ("contrast", "normal")])?
        .ready()
        .unwrap();
    assert_eq!(a, b);
    Ok(())
}
```

Both methods return `Missing` with a request for the root if it has not been provided; `normalize_selection` also returns `Missing` while a nested resolver is not loaded.

## Validating every context

`resolve` only looks at the selected contexts. `validate_complete()` walks every context of every modifier as if all were active: `Missing` with every document any context needs, then `Ready(())` once all are present and the resolver structure is valid. It does **not** resolve token values. To check values too, resolve every combination; see the [cookbook recipe](./cookbook.md#validate-every-theme-combination-in-ci).

## Resolver errors

Structural errors are `InvalidDocument` against the resolver unless noted, and every API that reads resolvers reports the same ones.

| Situation | Error message |
|---|---|
| `version` missing or not `2025.10` | ``resolver version must be `2025.10` `` |
| A member not allowed at its location | ``unknown resolver member `<name>` in <location>`` |
| `resolutionOrder` missing / empty | `resolutionOrder is required` / `resolutionOrder must not be empty` |
| `sets`, `modifiers`, a set, a modifier or a step is not an object | `resolver sets must be an object`, `set must be an object`, `resolution step must be an object` and so on |
| A `$ref` step not pointing at `#/sets/...` or `#/modifiers/...` | ``unknown resolver source `...` ``, or `InvalidPointer` |
| A `$ref` step to an undefined set or modifier | `InvalidPointer` |
| Inline step without `type` or `name`, or with another type | ``inline resolution step requires `type` `` / `` ...requires `name` `` / ``resolution step type must be `set` or `modifier` `` |
| Two inline steps with the same name | ``duplicate inline resolution name `...` `` |
| A set without `sources` | `set sources are required` |
| A modifier with no, one, or non-array contexts | `modifier requires contexts` / `modifier requires at least two contexts` / `modifier context must be an array` |
| A `default` that is not a string or not a context | `modifier default must be a string` / `modifier default context is unknown` |
| A source that is not an object | `document source must be an object` |
| A source reference into `resolutionOrder` or `modifiers` | ``forbidden source reference `...` `` |
| Local source references forming a cycle | ``reference cycle at `...` `` |
| An override on a reference whose target is not an object | `reference target must be an object` |
| A resolver that includes itself | `DocumentCycle` |

Next: [Loading documents](./loading.md).
