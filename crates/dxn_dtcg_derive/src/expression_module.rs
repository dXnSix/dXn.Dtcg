//! `#[expression_module]`: the public functions of an `impl` block as a
//! native module for `dxn_dtcg` expressions.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, Expr, ExprLit, FnArg, GenericArgument, ImplItem, ItemImpl, Lit, LitStr, Meta, Pat,
    PathArguments, ReturnType, Type, Visibility, ext::IdentExt,
};

const MODULE_KEYS: &[&str] = &["specifier"];
const FUNCTION_KEYS: &[&str] = &["name", "ts"];

pub(crate) fn expand(attribute: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let specifier = module_specifier(attribute)?;
    let mut block: ItemImpl = syn::parse2(item)?;
    if let Some(parameter) = block.generics.params.first() {
        return Err(syn::Error::new_spanned(
            parameter,
            "`#[expression_module]` does not support generic `impl` blocks",
        ));
    }
    if let Some((_, path, _)) = &block.trait_ {
        return Err(syn::Error::new_spanned(
            path,
            "`#[expression_module]` applies to an inherent `impl` block, not a trait implementation",
        ));
    }
    let self_type = &block.self_ty;
    let mut registrations = Vec::new();
    for item in &mut block.items {
        let ImplItem::Fn(function) = item else {
            continue;
        };
        let options = function_options(&function.attrs)?;
        function
            .attrs
            .retain(|attribute| !attribute.path().is_ident("expression"));
        if !matches!(function.vis, Visibility::Public(_)) {
            continue;
        }
        let signature = &function.sig;
        if let Some(asyncness) = &signature.asyncness {
            return Err(syn::Error::new_spanned(
                asyncness,
                "expression module functions are synchronous: remove `async`",
            ));
        }
        if let Some(parameter) = signature.generics.params.first() {
            return Err(syn::Error::new_spanned(
                parameter,
                "expression module functions cannot have generic parameters",
            ));
        }
        let rust_name = &signature.ident;
        let export = options
            .name
            .as_ref()
            .map_or_else(|| camel_case(&rust_name.unraw().to_string()), LitStr::value);
        if let Some(problem) = export_problem(&export) {
            let message = format!(
                "`{export}` {problem} and cannot name an export; rename it with `#[expression(name = \"…\")]`"
            );
            return Err(match &options.name {
                Some(name) => syn::Error::new_spanned(name, message),
                None => syn::Error::new_spanned(rust_name, message),
            });
        }
        let mut parameters = Vec::new();
        let mut arguments = Vec::new();
        let mut typescript = Vec::new();
        for (index, input) in signature.inputs.iter().enumerate() {
            let FnArg::Typed(typed) = input else {
                return Err(syn::Error::new_spanned(
                    input,
                    "expression module functions take no `self`: they are called without a value",
                ));
            };
            let binding = format_ident!("__argument{index}");
            let label = match typed.pat.as_ref() {
                Pat::Ident(pattern) => pattern.ident.unraw().to_string(),
                _ => format!("argument{index}"),
            };
            let (owned, borrowed) = owned_type(&typed.ty);
            parameters.push(quote! { #binding: #owned });
            arguments.push(if borrowed {
                quote! { &#binding }
            } else {
                quote! { #binding }
            });
            typescript.push(format!("{}: {}", camel_case(&label), ts_type(&typed.ty)));
        }
        let call = quote! { <#self_type>::#rust_name(#(#arguments),*) };
        let (body, returns) = match &signature.output {
            ReturnType::Default => (
                quote! { #call; ::core::result::Result::<(), ::core::convert::Infallible>::Ok(()) },
                "void".to_owned(),
            ),
            ReturnType::Type(_, ty) if result_inner(ty).is_some() => {
                (call.clone(), ts_type(result_inner(ty).expect("checked")))
            }
            ReturnType::Type(_, ty) => (
                quote! { ::core::result::Result::<_, ::core::convert::Infallible>::Ok(#call) },
                ts_type(ty),
            ),
        };
        let typescript = options.ts.as_ref().map_or_else(
            || format!("({}) => {returns}", typescript.join(", ")),
            LitStr::value,
        );
        let docs = documentation(&function.attrs);
        registrations.push(quote! {
            .function(#export, |#(#parameters),*| { #body })
            .describe(#export, #typescript, #docs)
        });
    }
    let module_docs = documentation(&block.attrs);
    Ok(quote! {
        #block

        impl #self_type {
            /// The native module of this block's public functions, for
            /// `ExpressionOptions::with_native_module`.
            pub fn expression_module() -> ::dxn_dtcg::expressions::NativeModule {
                ::dxn_dtcg::expressions::NativeModule::new(#specifier)
                    .with_docs(#module_docs)
                    #(#registrations)*
            }
        }
    })
}

fn module_specifier(attribute: TokenStream2) -> syn::Result<LitStr> {
    let mut specifier = None;
    let parser = syn::meta::parser(|meta| {
        let key = meta
            .path
            .get_ident()
            .map(|ident| ident.unraw().to_string())
            .unwrap_or_default();
        match key.as_str() {
            "specifier" => {
                if specifier
                    .replace(meta.value()?.parse::<LitStr>()?)
                    .is_some()
                {
                    return Err(meta.error("duplicate `specifier`"));
                }
                Ok(())
            }
            unknown => Err(meta.error(format!(
                "unknown `expression_module` attribute `{unknown}`; expected {}",
                MODULE_KEYS.join(", ")
            ))),
        }
    });
    syn::parse::Parser::parse2(parser, attribute)?;
    specifier.ok_or_else(|| {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "`#[expression_module]` needs a module specifier: `#[expression_module(specifier = \"…\")]`",
        )
    })
}

#[derive(Default)]
struct FunctionOptions {
    name: Option<LitStr>,
    ts: Option<LitStr>,
}

fn function_options(attributes: &[Attribute]) -> syn::Result<FunctionOptions> {
    let mut options = FunctionOptions::default();
    for attribute in attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("expression"))
    {
        attribute.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(|ident| ident.unraw().to_string())
                .unwrap_or_default();
            let slot = match key.as_str() {
                "name" => &mut options.name,
                "ts" => &mut options.ts,
                unknown => {
                    return Err(meta.error(format!(
                        "unknown `expression` attribute `{unknown}`; expected one of {}",
                        FUNCTION_KEYS.join(", ")
                    )));
                }
            };
            if slot.replace(meta.value()?.parse()?).is_some() {
                return Err(meta.error(format!("duplicate `expression` attribute `{key}`")));
            }
            Ok(())
        })?;
    }
    Ok(options)
}

/// The doc comments of an item, one line each, without the leading space.
fn documentation(attributes: &[Attribute]) -> String {
    attributes
        .iter()
        .filter_map(|attribute| match &attribute.meta {
            Meta::NameValue(pair) if pair.path.is_ident("doc") => match &pair.value {
                Expr::Lit(ExprLit {
                    lit: Lit::Str(text),
                    ..
                }) => Some(text.value()),
                _ => None,
            },
            _ => None,
        })
        .map(|line| line.strip_prefix(' ').map(str::to_owned).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Words that cannot name a binding in an ES module.
const RESERVED: &[&str] = &[
    "arguments",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "eval",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// Why `name` cannot be a JavaScript export binding, if it cannot.
fn export_problem(name: &str) -> Option<&'static str> {
    let mut chars = name.chars();
    let identifier = chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_' || first == '$')
        && chars.all(|next| next.is_alphanumeric() || next == '_' || next == '$');
    if !identifier {
        Some("is not a JavaScript identifier")
    } else if RESERVED.contains(&name) {
        Some("is a reserved word in JavaScript")
    } else {
        None
    }
}

/// `snake_case` to `camelCase`; other names stay as they are.
fn camel_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = false;
    for (index, character) in name.chars().enumerate() {
        if character == '_' && index > 0 {
            upper = true;
        } else if upper {
            out.extend(character.to_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

/// The type a closure parameter takes for an argument of type `ty`, and
/// whether the function borrows it: `&str` arrives as `String`, `&[T]` as
/// `Vec<T>` and `&T` as `T`.
fn owned_type(ty: &Type) -> (TokenStream2, bool) {
    let Type::Reference(reference) = ty else {
        return (quote! { #ty }, false);
    };
    match reference.elem.as_ref() {
        Type::Path(path) if path.path.is_ident("str") => (quote! { ::std::string::String }, true),
        Type::Slice(slice) => {
            let element = &slice.elem;
            (quote! { ::std::vec::Vec<#element> }, true)
        }
        inner => (quote! { #inner }, true),
    }
}

/// The `T` of `Result<T, E>` (any path ending in `Result` with type
/// arguments), the return form whose error becomes a JavaScript `Error`.
fn result_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let last = path.path.segments.last()?;
    if last.ident != "Result" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &last.arguments else {
        return None;
    };
    arguments.args.iter().find_map(|argument| match argument {
        GenericArgument::Type(ty) => Some(ty),
        _ => None,
    })
}

/// The DTCG value models of `dxn_dtcg::values` that (de)serialize as their
/// DTCG JSON, declared under the same names.
const VALUE_MODELS: &[&str] = &[
    "Color",
    "Dimension",
    "Duration",
    "CubicBezier",
    "FontFamily",
    "FontWeight",
    "StrokeStyle",
    "Border",
    "Transition",
    "Shadow",
    "Gradient",
    "Typography",
];

/// The TypeScript type of a Rust type, mapped by its spelling: a proc macro
/// cannot see trait implementations, so anything unknown is `unknown`.
fn ts_type(ty: &Type) -> String {
    match ty {
        Type::Reference(reference) => ts_type(&reference.elem),
        Type::Paren(inner) => ts_type(&inner.elem),
        Type::Group(inner) => ts_type(&inner.elem),
        Type::Slice(slice) => array(&slice.elem),
        Type::Array(array_type) => array(&array_type.elem),
        Type::Tuple(tuple) if tuple.elems.is_empty() => "void".into(),
        Type::Tuple(tuple) => format!(
            "[{}]",
            tuple
                .elems
                .iter()
                .map(ts_type)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Path(path) if path.qself.is_none() => {
            let Some(last) = path.path.segments.last() else {
                return "unknown".into();
            };
            let name = last.ident.to_string();
            let argument = match &last.arguments {
                PathArguments::AngleBracketed(arguments) => {
                    arguments.args.iter().find_map(|argument| match argument {
                        GenericArgument::Type(ty) => Some(ty),
                        _ => None,
                    })
                }
                _ => None,
            };
            match (name.as_str(), argument) {
                (
                    "f32" | "f64" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16"
                    | "u32" | "u64" | "u128" | "usize",
                    None,
                ) => "number".into(),
                ("String" | "str" | "char", None) => "string".into(),
                ("bool", None) => "boolean".into(),
                ("Option", Some(inner)) => format!("{} | null", ts_type(inner)),
                ("Vec", Some(inner)) => array(inner),
                ("Box" | "Rc" | "Arc", Some(inner)) => ts_type(inner),
                ("Value", None) => "unknown".into(),
                (model, None) if VALUE_MODELS.contains(&model) => model.into(),
                _ => "unknown".into(),
            }
        }
        _ => "unknown".into(),
    }
}

fn array(element: &Type) -> String {
    let element = ts_type(element);
    if element.contains(' ') {
        format!("({element})[]")
    } else {
        format!("{element}[]")
    }
}
