//! Derive macros projecting resolved DTCG tokens into Rust structs.
//!
//! Attributes are parsed strictly: unknown keys, duplicated keys, field
//! attributes on the struct and conflicting combinations are compile errors
//! spanned at the offending attribute. Generated code names every item by its
//! full path, so local aliases such as `type Result<T> = …` cannot change it.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Fields, GenericArgument, GenericParam, Ident, LitStr,
    PathArguments, Type, ext::IdentExt, parse_macro_input, spanned::Spanned,
};

mod expression_module;

/// Turns the public functions of an inherent `impl` block into a native
/// module for `dxn_dtcg` expressions (feature `expressions`): the block
/// gains `fn expression_module() -> dxn_dtcg::expressions::NativeModule`.
///
/// `#[expression_module(specifier = "brand/math")]` names the module. Each
/// `pub fn` becomes an export named in camelCase, or as
/// `#[expression(name = "…")]` says; its doc comment becomes its TSDoc, and
/// its TypeScript signature is mapped from the Rust types by their spelling
/// (`#[expression(ts = "(value: number) => number")]` overrides it).
/// Functions take no receiver, are not `async` and have no generic
/// parameters. Arguments deserialize from JSON (`&str` arrives as a
/// `String`, `&[T]` as a `Vec<T>`), results serialize, and a returned
/// `Result` fails with its error's `Display` text.
#[proc_macro_attribute]
pub fn expression_module(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expression_module::expand(attribute.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(DtcgTokens, attributes(dtcg))]
pub fn derive_dtcg_tokens(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input, false)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_derive(DtcgComponentTokens, attributes(dtcg))]
pub fn derive_dtcg_component_tokens(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input, true)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

const FIELD_KEYS: &[&str] = &["path", "rename", "with", "nested", "rest"];
const STRUCT_KEYS: &[&str] = &["root", "id"];

fn expand(input: DeriveInput, component: bool) -> syn::Result<TokenStream2> {
    let derive = if component {
        "DtcgComponentTokens"
    } else {
        "DtcgTokens"
    };
    let name = input.ident.clone();
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &name,
            format!("{derive} only supports structs"),
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &name,
            format!("{derive} requires named fields"),
        ));
    };
    let attributes = struct_attributes(&input.attrs, component)?;
    let root = attributes
        .root
        .map_or_else(String::new, |root| root.value());
    let component_id = attributes
        .id
        .map_or_else(|| name.to_string(), |id| id.value());

    let parameters = input
        .generics
        .params
        .iter()
        .filter_map(|parameter| match parameter {
            GenericParam::Type(parameter) => Some(parameter.ident.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut generics = input.generics.clone();
    let mut rest_field: Option<Ident> = None;
    let mut initializers = Vec::new();
    for field in &fields.named {
        let ident = field.ident.clone().expect("named field");
        let attributes = field_attributes(&field.attrs)?;
        let ty = &field.ty;
        let generic = mentions_any(ty, &parameters);
        if attributes.rest.is_some() {
            if rest_field.replace(ident.clone()).is_some() {
                return Err(syn::Error::new_spanned(
                    &ident,
                    format!("{derive} permits exactly one #[dtcg(rest)] field"),
                ));
            }
            if generic {
                generics
                    .make_where_clause()
                    .predicates
                    .push(syn::parse_quote!(#ty: ::core::default::Default));
            }
            initializers.push(quote! { #ident: ::core::default::Default::default() });
            continue;
        }
        let path = attributes
            .path
            .or(attributes.rename)
            .map_or_else(|| ident.unraw().to_string(), |path| path.value());
        if attributes.nested.is_some() {
            if generic {
                generics
                    .make_where_clause()
                    .predicates
                    .push(syn::parse_quote!(#ty: ::dxn_dtcg::DtcgTokens));
            }
            initializers.push(quote! {
                #ident: <#ty as ::dxn_dtcg::DtcgTokens>::project_at(
                    tokens,
                    registry,
                    &::dxn_dtcg::join_token_path(&base, #path),
                )?
            });
            continue;
        }
        let optional = option_inner(ty);
        let value_type = optional.unwrap_or(ty);
        let convert = match &attributes.with {
            Some(with) => quote! { #with(token)? },
            None => {
                if mentions_any(value_type, &parameters) {
                    generics
                        .make_where_clause()
                        .predicates
                        .push(syn::parse_quote!(#value_type: ::dxn_dtcg::FromDtcgValue));
                }
                quote! { registry.convert::<#value_type>(token)? }
            }
        };
        let initializer = if optional.is_some() {
            quote! {
                #ident: {
                    let full_path = ::dxn_dtcg::join_token_path(&base, #path);
                    match tokens.get(&full_path) {
                        ::core::option::Option::Some(token) => ::core::option::Option::Some(#convert),
                        ::core::option::Option::None => ::core::option::Option::None,
                    }
                }
            }
        } else {
            quote! {
                #ident: {
                    let full_path = ::dxn_dtcg::join_token_path(&base, #path);
                    let token = tokens.get(&full_path).ok_or_else(|| ::dxn_dtcg::DtcgError::Projection {
                        path: ::core::clone::Clone::clone(&full_path),
                        message: ::core::convert::Into::into("required token is missing"),
                    })?;
                    #convert
                }
            }
        };
        initializers.push(initializer);
    }

    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let component_impl = component.then(|| {
        quote! {
            impl #impl_generics ::dxn_dtcg::DtcgComponentTokens for #name #type_generics #where_clause {
                fn component_id() -> &'static str {
                    #component_id
                }
            }
        }
    });
    Ok(quote! {
        impl #impl_generics ::dxn_dtcg::DtcgTokens for #name #type_generics #where_clause {
            fn project_at(
                tokens: &::dxn_dtcg::ResolvedTokens,
                registry: &::dxn_dtcg::TypeRegistry,
                root: &str,
            ) -> ::core::result::Result<Self, ::dxn_dtcg::DtcgError> {
                let base = ::dxn_dtcg::join_token_path(root, #root);
                let _ = (&base, tokens, registry);
                ::core::result::Result::Ok(Self { #(#initializers),* })
            }
        }
        #component_impl
    })
}

#[derive(Default)]
struct StructAttributes {
    root: Option<LitStr>,
    id: Option<LitStr>,
}

fn struct_attributes(attributes: &[Attribute], component: bool) -> syn::Result<StructAttributes> {
    let mut result = StructAttributes::default();
    for attribute in dtcg_attributes(attributes) {
        attribute.parse_nested_meta(|meta| {
            let key = key_name(&meta)?;
            match key.as_str() {
                "root" => set_once(&mut result.root, meta.value()?.parse()?, &meta, "root"),
                "id" if component => set_once(&mut result.id, meta.value()?.parse()?, &meta, "id"),
                "id" => {
                    Err(meta.error("`id` is only supported by `#[derive(DtcgComponentTokens)]`"))
                }
                field if FIELD_KEYS.contains(&field) => Err(meta.error(format!(
                    "`{field}` is a field attribute and cannot be used on the struct"
                ))),
                unknown => Err(meta.error(format!(
                    "unknown `dtcg` struct attribute `{unknown}`; expected one of {}",
                    STRUCT_KEYS.join(", ")
                ))),
            }
        })?;
    }
    Ok(result)
}

#[derive(Default)]
struct FieldAttributes {
    path: Option<LitStr>,
    rename: Option<LitStr>,
    with: Option<syn::Path>,
    nested: Option<Span>,
    rest: Option<Span>,
}

fn field_attributes(attributes: &[Attribute]) -> syn::Result<FieldAttributes> {
    let mut result = FieldAttributes::default();
    let mut spans = Vec::<(String, Span)>::new();
    for attribute in dtcg_attributes(attributes) {
        attribute.parse_nested_meta(|meta| {
            let key = key_name(&meta)?;
            let span = meta.path.span();
            match key.as_str() {
                "path" => set_once(&mut result.path, meta.value()?.parse()?, &meta, "path")?,
                "rename" => set_once(&mut result.rename, meta.value()?.parse()?, &meta, "rename")?,
                "with" => {
                    let literal: LitStr = meta.value()?.parse()?;
                    let path = literal.parse::<syn::Path>().map_err(|_| {
                        syn::Error::new(
                            literal.span(),
                            "`with` must name a conversion function, such as \"crate::convert\"",
                        )
                    })?;
                    set_once(&mut result.with, path, &meta, "with")?
                }
                "nested" | "rest" => {
                    if meta.input.peek(syn::Token![=]) {
                        return Err(meta.error(format!("`{key}` takes no value")));
                    }
                    let flag = if key == "nested" {
                        &mut result.nested
                    } else {
                        &mut result.rest
                    };
                    set_once(flag, span, &meta, &key)?
                }
                field if STRUCT_KEYS.contains(&field) => {
                    return Err(meta.error(format!(
                        "`{field}` is a struct attribute and cannot be used on a field"
                    )));
                }
                unknown => {
                    return Err(meta.error(format!(
                        "unknown `dtcg` field attribute `{unknown}`; expected one of {}",
                        FIELD_KEYS.join(", ")
                    )));
                }
            }
            spans.push((key, span));
            Ok(())
        })?;
    }
    let span_of = |key: &str| {
        spans
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, span)| *span)
    };
    if result.path.is_some() && result.rename.is_some() {
        let later = spans
            .iter()
            .rev()
            .find(|(key, _)| key == "path" || key == "rename")
            .map_or_else(Span::call_site, |(_, span)| *span);
        return Err(syn::Error::new(
            later,
            "`path` and `rename` cannot be combined: both set the token path",
        ));
    }
    if result.nested.is_some() && result.with.is_some() {
        let span = span_of("with").unwrap_or_else(Span::call_site);
        return Err(syn::Error::new(
            span,
            "`nested` and `with` cannot be combined: a nested field projects a struct",
        ));
    }
    if result.rest.is_some() {
        for other in ["path", "rename", "with", "nested"] {
            if let Some(span) = span_of(other) {
                return Err(syn::Error::new(
                    span,
                    format!("`rest` cannot be combined with `{other}`"),
                ));
            }
        }
    }
    Ok(result)
}

fn dtcg_attributes(attributes: &[Attribute]) -> impl Iterator<Item = &Attribute> {
    attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("dtcg"))
}

fn key_name(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<String> {
    meta.path
        .get_ident()
        .map(|ident| ident.unraw().to_string())
        .ok_or_else(|| meta.error("expected a `dtcg` attribute name"))
}

fn set_once<T>(
    slot: &mut Option<T>,
    value: T,
    meta: &syn::meta::ParseNestedMeta<'_>,
    key: &str,
) -> syn::Result<()> {
    if slot.replace(value).is_some() {
        return Err(meta.error(format!("duplicate `dtcg` attribute `{key}`")));
    }
    Ok(())
}

/// The `T` of a field typed `Option<T>`, `core::option::Option<T>` or
/// `std::option::Option<T>` (with or without a leading `::`).
fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    if path.qself.is_some() {
        return None;
    }
    let segments = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    let is_option = match segments.as_slice() {
        [option] => option == "Option" && path.path.leading_colon.is_none(),
        [krate, module, option] => {
            (krate == "core" || krate == "std") && module == "option" && option == "Option"
        }
        _ => false,
    };
    if !is_option {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &path.path.segments.last()?.arguments else {
        return None;
    };
    match arguments.args.iter().collect::<Vec<_>>().as_slice() {
        [GenericArgument::Type(inner)] => Some(inner),
        _ => None,
    }
}

/// Whether `ty` mentions any of the struct's type parameters.
fn mentions_any(ty: &Type, parameters: &[Ident]) -> bool {
    fn visit(tokens: TokenStream2, parameters: &[Ident]) -> bool {
        tokens.into_iter().any(|token| match token {
            TokenTree::Ident(ident) => parameters.contains(&ident),
            TokenTree::Group(group) => visit(group.stream(), parameters),
            _ => false,
        })
    }
    !parameters.is_empty() && visit(quote!(#ty), parameters)
}
