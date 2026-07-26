use proc_macro::TokenStream;
use quote::quote;
use syn::{Attribute, Data, DeriveInput, Fields, GenericParam, LitStr, parse_macro_input};

#[proc_macro_derive(DtcgTokens, attributes(dtcg))]
pub fn derive_dtcg_tokens(input: TokenStream) -> TokenStream {
    derive(input, false)
}

#[proc_macro_derive(DtcgComponentTokens, attributes(dtcg))]
pub fn derive_dtcg_component_tokens(input: TokenStream) -> TokenStream {
    derive(input, true)
}

fn derive(input: TokenStream, component: bool) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident.clone();
    let Data::Struct(data) = input.data else {
        return syn::Error::new_spanned(name, "DtcgTokens only supports structs")
            .to_compile_error()
            .into();
    };
    let Fields::Named(fields) = data.fields else {
        return syn::Error::new_spanned(name, "DtcgTokens requires named fields")
            .to_compile_error()
            .into();
    };
    let root = match attribute_value(&input.attrs, "root") {
        Ok(value) => value.unwrap_or_default(),
        Err(error) => return error.to_compile_error().into(),
    };
    let component_id = match attribute_value(&input.attrs, "id") {
        Ok(value) => value.unwrap_or_else(|| name.to_string()),
        Err(error) => return error.to_compile_error().into(),
    };
    let mut rest_field = None;
    let mut initializers = Vec::new();
    for field in fields.named {
        let Some(ident) = field.ident else { continue };
        let attributes = match field_attributes(&field.attrs) {
            Ok(attributes) => attributes,
            Err(error) => return error.to_compile_error().into(),
        };
        if attributes.rest {
            if rest_field.replace(ident.clone()).is_some() {
                return syn::Error::new_spanned(
                    ident,
                    "DtcgTokens permits exactly one #[dtcg(rest)] field",
                )
                .to_compile_error()
                .into();
            }
            initializers.push(quote! { #ident: ::core::default::Default::default() });
            continue;
        }
        let ty = field.ty;
        let path = attributes
            .path
            .unwrap_or_else(|| attributes.rename.unwrap_or_else(|| ident.to_string()));
        let full_path = if root.is_empty() {
            path
        } else if path.is_empty() {
            root.clone()
        } else {
            format!("{root}.{path}")
        };
        if attributes.nested {
            initializers.push(
                quote! { #ident: <#ty as ::dtcg_runtime::DtcgTokens>::project(tokens, registry)? },
            );
        } else {
            initializers.push(quote! { #ident: { let token = tokens.get(#full_path).ok_or_else(|| ::dtcg_runtime::DtcgError::Projection { path: #full_path.into(), message: "required token is missing".into() })?; registry.convert::<#ty>(token)? } });
        }
    }
    let mut generics = input.generics;
    for parameter in &mut generics.params {
        if let GenericParam::Type(parameter) = parameter {
            parameter
                .bounds
                .push(syn::parse_quote!(::dtcg_runtime::DtcgTokens));
        }
    }
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let component_impl = component.then(|| quote! { impl #impl_generics ::dtcg_runtime::DtcgComponentTokens for #name #type_generics #where_clause { fn component_id() -> &'static str { #component_id } } });
    quote! {
        impl #impl_generics ::dtcg_runtime::DtcgTokens for #name #type_generics #where_clause {
            fn project(tokens: &::dtcg_runtime::ResolvedTokens, registry: &::dtcg_runtime::TypeRegistry) -> Result<Self, ::dtcg_runtime::DtcgError> {
                Ok(Self { #(#initializers),* })
            }
        }
        #component_impl
    }.into()
}

#[derive(Default)]
struct FieldAttributes {
    path: Option<String>,
    rename: Option<String>,
    nested: bool,
    rest: bool,
}

fn field_attributes(attributes: &[Attribute]) -> syn::Result<FieldAttributes> {
    let mut result = FieldAttributes::default();
    for attribute in attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("dtcg"))
    {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("nested") {
                result.nested = true;
                return Ok(());
            }
            if meta.path.is_ident("rest") {
                result.rest = true;
                return Ok(());
            }
            let value = meta.value()?.parse::<LitStr>()?.value();
            if meta.path.is_ident("path") {
                result.path = Some(value);
                Ok(())
            } else if meta.path.is_ident("rename") {
                result.rename = Some(value);
                Ok(())
            } else {
                Err(meta.error("unsupported dtcg field attribute"))
            }
        })?;
    }
    Ok(result)
}

fn attribute_value(attributes: &[Attribute], name: &str) -> syn::Result<Option<String>> {
    let mut result = None;
    for attribute in attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("dtcg"))
    {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident(name) {
                result = Some(meta.value()?.parse::<LitStr>()?.value());
                Ok(())
            } else {
                if meta.input.peek(syn::Token![=]) {
                    let _ = meta.value()?.parse::<LitStr>()?;
                }
                Ok(())
            }
        })?;
    }
    Ok(result)
}
