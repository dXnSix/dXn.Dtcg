# 6. Resolvers and modifiers

A resolver document says which token sources make up a design system, in what order to merge
them, and which variants can be switched at run time. This chapter covers the resolver format as
`dxn_dtcg` implements it (DTCG Resolver 2025.10), how sources are found and merged, and how
selections pick contexts.

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
token document, even one with a top-level group named `modifiers` or `sets`. That applies to the
root and to every source:

| Root | Behavior |
|---|---|
| A token document | It is the only source. `modifiers()` returns no axes, and selections are ignored. |
| A resolver | Its `resolutionOrder` decides which sources are merged, and selections pick modifier contexts. |

A source inside a resolver can itself be a resolver, inline or in another file. See
[nested resolvers](#nested-resolvers).

## Resolver structure

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

A resolver may contain only the members Resolver §4 defines. Anything else fails with
``unknown resolver member `<name>` in <location>``:

| Location | Allowed members |
|---|---|
| The root | `version` (required, exactly `"2025.10"`), `resolutionOrder` (required, a non-empty array), `name`, `description`, `sets`, `modifiers`, `$schema`, `$defs` |
| A set in `sets` | `sources` (required), `description`, `$extensions` |
| A modifier in `modifiers` | `contexts` (required), `default`, `description`, `$extensions` |
| An inline set step | `type`, `name`, `sources`, `description`, `$extensions` |
| An inline modifier step | `type`, `name`, `contexts`, `default`, `description`, `$extensions` |
| A `$ref` step or source reference object | `$ref` and any other members, which are [overrides](#reference-objects-with-overrides) |

The root does not accept `$extensions` or `x-*` members. Put shared JSON under `$defs` instead and
reach it with local pointer sources such as `{ "$ref": "#/$defs/motion" }`. `$schema` is accepted
and ignored.

The same rules apply to the root, to [nested resolvers](#nested-resolvers) and to the `data` of a
[Token Editor envelope](07-document-loading.md#token-editor-envelopes). A resolver is checked when
discovery reaches it, which means on every call that involves it.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn structure_error(resolver: &str) -> String {
    let runtime = DtcgRuntime::from_json("r.json", resolver).unwrap();
    match runtime.resolve(&[]) {
        Err(DtcgError::InvalidDocument { message, .. }) => message,
        other => panic!("expected InvalidDocument, got {other:?}"),
    }
}

fn main() -> Result<(), DtcgError> {
    // `$schema`, `$defs`, `name` and `description` are fine.
    let runtime = DtcgRuntime::from_json(
        "r.json",
        r##"{
          "$schema": "https://www.designtokens.org/schemas/2025.10/resolver.json",
          "version": "2025.10",
          "name": "Acme",
          "$defs": { "base": { "gap": { "$type": "number", "$value": 8 } } },
          "resolutionOrder": [{ "type": "set", "name": "base", "sources": [{ "$ref": "#/$defs/base" }] }]
        }"##,
    )?;
    assert_eq!(runtime.resolve(&[])?.ready().unwrap().get("gap").unwrap().value, 8);

    assert_eq!(
        structure_error(r#"{ "version": "2025.10", "$extensions": {}, "resolutionOrder": [{ "type": "set", "name": "s", "sources": [] }] }"#),
        "unknown resolver member `$extensions` in the resolver root"
    );
    assert_eq!(
        structure_error(r##"{ "version": "2025.10", "sets": { "base": { "sources": [], "tokens": {} } }, "resolutionOrder": [{ "$ref": "#/sets/base" }] }"##),
        "unknown resolver member `tokens` in set `base`"
    );
    assert_eq!(
        structure_error(r#"{ "version": "2025.10", "resolutionOrder": [] }"#),
        "resolutionOrder must not be empty"
    );
    Ok(())
}
```

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

- `contexts` is required: an object with **at least two** members (Resolver §4.2), each an array of
  sources. A context array may be empty, which is how a context says "nothing changes". A modifier
  with one context fails with `modifier requires at least two contexts`; use a set for sources that
  are always applied.
- `default` is optional. If present, it must name one of the contexts. A modifier without a default
  must be selected explicitly.
- Only the selected context's sources are applied, and only its documents are requested. The other
  contexts are inactive.
- Context order is the authored order. `modifiers()` reports contexts in that order.

> Every root modifier is an input, even one that `resolutionOrder` never references. It is listed
> by `modifiers()` and needs a default or a selection, but contributes no sources.

## `resolutionOrder`

`resolutionOrder` is a non-empty array of steps, applied first to last. A step is one of:

| Step | Example |
|---|---|
| A reference to a set | `{ "$ref": "#/sets/foundation" }` |
| A reference to a modifier | `{ "$ref": "#/modifiers/theme" }` |
| A reference with overrides | `{ "$ref": "#/modifiers/theme", "default": "dark" }` |
| An inline set | `{ "type": "set", "name": "overrides", "sources": [ … ] }` |
| An inline modifier | `{ "type": "modifier", "name": "density", "default": "comfortable", "contexts": { … } }` |

Rules:

- Each step is an object. A `$ref` step must point to `#/sets/<name>` or `#/modifiers/<name>`, and
  the definition must exist. Names in pointers use JSON Pointer escaping (`~1` for `/`, `~0` for
  `~`) and may be percent-encoded.
- **Step overrides.** Members next to `$ref` replace the definition's top-level members, as for
  [reference objects](#reference-objects-with-overrides). The result is the step's one effective
  definition, and every API uses it: `resolve`, `validate_complete`, selection checking,
  `modifiers()`, `normalize_selection` and the inspection APIs.
- An inline step needs `type` (`"set"` or `"modifier"`) and a non-empty `name`. Inline names must
  be unique within one `resolutionOrder`.
- An inline modifier is a modifier like any other: it appears in `modifiers()` and is selected by
  its `name`.
- A modifier may be defined by only one step. Two steps that define the same modifier name (two
  references to it, or a reference and an inline step) fail with
  ``resolutionOrder defines modifier `<name>` more than once``.
- The same set may be referenced by more than one step. It is then applied at each position.

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

    // A modifier defined by two steps is an error for every API.
    let twice = DtcgRuntime::from_json(
        "r.json",
        r##"{
          "version": "2025.10",
          "modifiers": { "theme": { "default": "light", "contexts": { "light": [], "dark": [] } } },
          "resolutionOrder": [
            { "$ref": "#/modifiers/theme" },
            { "type": "modifier", "name": "theme", "default": "a", "contexts": { "a": [], "b": [] } }
          ]
        }"##,
    )?;
    assert!(matches!(
        twice.modifiers(),
        Err(DtcgError::InvalidDocument { message, .. })
            if message == "resolutionOrder defines modifier `theme` more than once"
    ));
    Ok(())
}
```

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

Fragments are JSON Pointers and are percent-decoded first (RFC 6901 §6), so
`library.json#/brand%20colors` addresses the member `brand colors`; see
[Document loading](07-document-loading.md#fragments). Local references into `#/resolutionOrder/…`
or `#/modifiers/…` are not allowed as sources, whatever their spelling, and local references must
not form a cycle.

A string `$schema` member is accepted and ignored at the root of a whole external document. Inline
sources and fragment sources are not document roots, so `$schema` there is an unknown reserved
property.

This example uses every kind except nested resolvers:

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
not merge into it. The same rule applies to `$ref` steps in `resolutionOrder` and to references
whose target is a resolver, where an override such as `"sets": {…}` replaces the target's `sets`.

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
produces take the place of the reference. This lets one design system build on another. Nested
resolvers are a runtime extension; Resolver 2025.10 does not define them.

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

Rules:

- **Inputs are shared by the whole graph.** The same `&[(modifier, context)]` is offered to the
  root and to every nested resolver that discovery reaches. An input is valid when **any** reached
  resolver declares it; each resolver applies the inputs that name its own modifiers and uses its
  defaults for the rest. An input that no reached resolver declares fails with `UnknownModifier`.
- **Missing inputs are per resolver.** A resolver requires an input only for its own modifiers
  that have no default.
- **Missing documents come first.** While a nested resolver is not yet loaded, its modifiers are
  unknown, so `resolve` returns `Missing` instead of reporting an unknown input.
- **Cycles.** A resolver that, directly or indirectly, includes itself fails with
  `DtcgError::DocumentCycle`. Only the chain of resolvers being expanded counts, so including the
  same resolver at two different positions is fine; it is expanded at both.
- **Limits.** The nesting depth is bounded by `RuntimeLimits::max_reference_depth` (default 64),
  and each expanded resolver counts toward `max_documents`.

`modifiers()` describes only the root resolver; `normalize_selection` covers the whole graph. See
[Discovering modifiers for a UI](#discovering-modifiers-for-a-ui).

## Overlay semantics

After discovery, the runtime has an ordered list of terminal token sources. It folds them into
**one merged token structure** (Resolver §6.2–§6.3) and resolves that structure as if it were a
single document:

1. Sources are folded in discovery order: `resolutionOrder` steps first to last, each set's or
   context's sources in array order, with references and nested resolvers expanded in place.
2. **Groups merge member by member.** A later source can add one token to an existing group
   without repeating the others.
3. **Group properties merge per key.** A later `$type`, `$description`, `$deprecated` or
   `$extends` replaces the earlier value, and an omitted one keeps it. Group `$extensions` objects
   merge recursively.
4. **Tokens are replaced whole.** A later token at the same path keeps nothing of the earlier one:
   not its value, `$type`, `$description`, `$deprecated`, `$extensions` or `source`.
5. **Tokens and groups do not mix.** A path that is a token in one source and a group in another
   fails with a `Token` error: ``path is a token in <a> and a group in <b>``.
6. Type inheritance, `$deprecated` inheritance, `$extends` and every JSON Pointer are evaluated on
   the merged structure, then aliases are resolved and values validated (see
   [Aliases see the final token set](04-aliases-and-references.md#aliases-see-the-final-token-set)).

Consequences for override files:

- **An override inherits its group's type.** `{ "size": { "lg": { "$value": 12 } } }` layered on
  a file that declares `"size": { "$type": "number", … }` is a `number` token. A group `$type` in
  the override file replaces the inherited one for every token in that group.
- **`$extends` and pointers see every source.** A group can extend a group from another source,
  and a pointer such as `#/$extensions/palette/red` reads the merged value, where a later source
  wins.
- **Composite values are replaced, not merged.** To change only the width of a border, the
  override must still give the full `{ color, width, style }` value. It cannot borrow members from
  the token it replaces (that would be an alias to itself), but it can alias the palette and size
  tokens the original was built from.
- **Tokens cannot be removed.** Later sources can add and replace, but not delete.

**Token order** is the authored order, with each path at the position of its last definition (see
[token order](03-token-documents.md#token-order)). `ResolvedTokens::iter()` follows it. Tokens
copied by `$extends` take the position of the extending group's `$extends` and name its document as
their `source`.

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime};

fn resolver(override_source: &str) -> String {
    format!(
        r##"{{
          "version": "2025.10",
          "resolutionOrder": [
            {{ "type": "set", "name": "base", "sources": [
              {{ "size": {{ "$type": "number", "$description": "Spacing scale",
                 "sm": {{ "$value": 4 }},
                 "lg": {{ "$value": 16, "$description": "Large" }} }} }}
            ] }},
            {{ "type": "set", "name": "compact", "sources": [{override_source}] }}
          ]
        }}"##
    )
}

fn main() -> Result<(), DtcgError> {
    let runtime = DtcgRuntime::from_json(
        "r.json",
        resolver(r#"{ "size": { "lg": { "$value": 12 }, "xl": { "$value": 20 } } }"#),
    )?;
    let tokens = runtime.resolve(&[])?.ready().unwrap();

    // The override inherits `number` from the base file's group.
    let lg = tokens.get("size.lg").unwrap();
    assert_eq!(lg.token_type.as_deref(), Some("number"));
    assert_eq!(lg.value, 12);
    assert_eq!(lg.description, None); // replaced whole: the base `$description` is gone
    assert_eq!(tokens.get("size.xl").unwrap().token_type.as_deref(), Some("number"));
    assert_eq!(tokens.get("size.sm").unwrap().value, 4); // untouched tokens remain
    assert_eq!(tokens.group("size").unwrap().description.as_deref(), Some("Spacing scale"));

    // Each path sits at its last definition.
    let paths: Vec<&str> = tokens.iter().map(|token| &*token.path).collect();
    assert_eq!(paths, ["size.sm", "size.lg", "size.xl"]);

    // A token where an earlier source has a group is an error.
    let collision = DtcgRuntime::from_json(
        "r.json",
        resolver(r#"{ "size": { "$type": "number", "$value": 1 } }"#),
    )?;
    assert!(matches!(
        collision.resolve(&[]),
        Err(DtcgError::Token { message, .. }) if message.starts_with("path is a token in")
    ));
    Ok(())
}
```

## Selections

A selection is a slice of `(modifier, context)` pairs (Resolver §5):

```rust
use dxn_dtcg::DtcgRuntime;

fn example(runtime: &DtcgRuntime) {
    let _ = runtime.resolve(&[]); // every modifier at its default
    let _ = runtime.resolve(&[("theme", "dark")]);
    let _ = runtime.resolve(&[("theme", "dark"), ("density", "compact")]); // order does not matter
}
```

- **Names match case-insensitively.** An exact match wins; otherwise names are compared after
  `str::to_lowercase`. `("Theme", "DARK")` selects `theme` / `dark`. When an input matches several
  declared names case-insensitively and none exactly, it is an error.
- Modifiers left out use their `default`. A modifier without a default must be selected.
- Naming a modifier no reached resolver declares, a context the modifier does not have, or the
  same modifier twice (also in a different case) is an error.
- A graph without modifiers, including a root that is a plain token document, ignores the
  selection.

`resolve` and `normalize_selection` validate selections the same way and return the same errors.
Both list the allowed values, so a user-facing picker can show them:

| Selection problem | Error |
|---|---|
| Unknown modifier | `UnknownModifier`: ``unknown modifier `size`; known modifiers: theme, density`` (declaration order, root first) |
| Modifier name matches several declared names | `UnknownModifier` whose `known` lists the candidates |
| Unknown context | `UnknownContext`: ``modifier `theme` context `blue`: selected modifier context is unknown; allowed contexts: light, dark`` |
| Context matches several declared contexts | `UnknownContext` with message `context matches more than one declared context` |
| No default, not selected | `UnknownContext` with an empty `context`: ```modifier `density` context ``: modifier has no default and was not selected; allowed contexts: comfortable, compact``` |
| Selected twice | `UnknownContext`: ``modifier `theme` context `light`: modifier is selected twice; allowed contexts: light, dark`` |

Selections that arrive as JSON, such as resolver inputs `{"theme": "dark"}` from a URL or a
configuration file, convert with `selection_from_json`. It returns owned pairs; `selection_refs`
borrows them as the `&[(&str, &str)]` every method takes. Anything but an object of strings fails
with `DtcgError::InvalidInput`.

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
    assert_eq!(runtime.normalize_selection(&[("theme", "dark")]).unwrap_err(), error);

    // An unknown modifier lists the declared ones.
    assert!(matches!(
        runtime.resolve(&[("size", "large"), ("density", "compact")]),
        Err(DtcgError::UnknownModifier { known, .. }) if known == ["theme", "density"]
    ));

    // JSON inputs.
    let inputs = selection_from_json(&json!({ "theme": "dark", "density": "compact" }))?;
    assert!(runtime.resolve(&selection_refs(&inputs))?.is_ready());
    assert!(matches!(
        selection_from_json(&json!({ "theme": 1 })),
        Err(DtcgError::InvalidInput { .. })
    ));
    Ok(())
}
```

## Discovering modifiers for a UI

`DtcgRuntime::modifiers()` describes the **root** resolver's inputs as `ModifierAxis` values:

```rust,ignore
#[non_exhaustive]
pub struct ModifierAxis {
    pub name: String,            // the modifier name to use in a selection
    pub contexts: Vec<String>,   // its contexts, in authored order
    pub default: Option<String>, // its default context, if any
}
```

Axes are listed in `resolutionOrder` order (referenced and inline modifiers alike, with step
overrides applied), followed by the root modifiers that `resolutionOrder` does not mention, in
authored order. Nested resolvers' modifiers are not listed, and `modifiers()` never loads any
document other than the root.

`normalize_selection` validates a selection exactly as `resolve` does, across every resolver the
graph reaches, and returns the chosen context of **every reached modifier**, nested ones included,
in declaration order (root first) with the declared spellings. Two selections that mean the same
thing normalize to equal vectors, which makes the result a good cache key for resolved tokens.

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
    let pairs: Vec<(&str, &str)> = a.iter().map(|(m, c)| (m.as_str(), c.as_str())).collect();
    assert_eq!(pairs, [("density", "comfortable"), ("theme", "dark"), ("contrast", "normal")]);

    let error = runtime.normalize_selection(&[("theme", "sepia")]).unwrap_err();
    assert!(matches!(error, DtcgError::UnknownContext { ref allowed, .. } if allowed == &["light", "dark"]));
    Ok(())
}
```

Both methods return `Resolution::Missing` with a request for the root document if it has not been
provided yet. `normalize_selection` also returns `Missing` while a nested resolver is not loaded.

## Validating every context

`resolve` only looks at the selected contexts. Two tools cover the rest.

**`validate_complete()`** walks every context of every modifier, as if all were active. It returns
`Missing` with every document any context needs, and `Ready(())` once all are present and the
resolver structure is valid. It does **not** resolve or validate token values. Use it to preload
everything, for example before going offline or bundling assets.

**Resolving every combination** checks the token values too. With a few modifiers, the cartesian
product of their contexts is small:

```rust
use dxn_dtcg::{DtcgError, DtcgRuntime, ModifierAxis};

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

`modifiers()` lists only the root's axes. For a graph with nested resolvers, add the nested
resolvers' modifiers to the combinations, or rely on their defaults.

In a CI job for a token repository, run this over every combination and fail on any error.

## Errors

Structural errors are `InvalidDocument` against the resolver unless noted. `resolve`,
`validate_complete`, `modifiers()`, `normalize_selection`, `inspect_source`, `validate_source`,
`source_references` and the previews report the same structural errors, because they read
resolvers through one definition.

| Situation | Error |
|---|---|
| `version` missing or not `2025.10` | ``resolver version must be `2025.10` `` |
| A member that Resolver §4 does not allow at its location | ``unknown resolver member `<name>` in <location>`` |
| `resolutionOrder` missing or not an array | `resolutionOrder is required` |
| `resolutionOrder` empty | `resolutionOrder must not be empty` |
| `sets` or `modifiers` not an object | `resolver sets must be an object` / `resolver modifiers must be an object` |
| A set or modifier that is not an object | `set must be an object` / `modifier must be an object` |
| A step that is not an object | `resolution step must be an object` |
| A `$ref` step that does not point to `#/sets/…` or `#/modifiers/…` | ``unknown resolver source `…` ``, or `InvalidPointer` when it is not a local `#/…` pointer |
| A `$ref` step to a set or modifier that is not defined | `InvalidPointer` |
| An inline step without `type` or `name`, or another `type` | ``inline resolution step requires `type` `` / ``inline resolution step requires `name` `` / ``resolution step type must be `set` or `modifier` `` |
| Two inline steps with the same name | ``duplicate inline resolution name `…` `` |
| Two steps that define the same modifier | ``resolutionOrder defines modifier `…` more than once`` |
| A set without a `sources` array | `set sources are required` |
| A modifier without contexts, with one context, or a context that is not an array | `modifier requires contexts` / `modifier requires at least two contexts` / `modifier context must be an array` |
| A `default` that is not a string or not a context | `modifier default must be a string` / `modifier default context is unknown` |
| A source that is not an object | `document source must be an object` |
| A source reference to `#/resolutionOrder/…` or `#/modifiers/…` | ``forbidden source reference `…` `` |
| A source reference to an unknown set | ``unknown set `…` `` |
| Local source references that form a cycle | ``reference cycle at `…` `` |
| An override on a reference whose target is not an object | `reference target must be an object` |
| A fragment pointer that does not exist in the target | `InvalidPointer` |
| A path that is a token in one source and a group in another | `Token`: ``path is a token in <a> and a group in <b>`` |
| A resolver that includes itself | `DocumentCycle` |
| A malformed or escaping reference path | `InvalidResourceId` / `InvalidPointer` |
| Selection problems | `UnknownModifier` / `UnknownContext`; see [Selections](#selections) |
