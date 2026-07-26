use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

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
    let name = input.ident;
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
    let fields = fields.named.iter().map(|field| {
        let Some(ident) = field.ident.as_ref() else { return quote! { compile_error!("DtcgTokens requires named fields"); }; };
        let ty = &field.ty;
        let mut path = ident.to_string();
        for attr in &field.attrs { if attr.path().is_ident("dtcg") { let _ = attr.parse_nested_meta(|meta| { if meta.path.is_ident("path") { let value = meta.value()?.parse::<syn::LitStr>()?; path = value.value(); } Ok(()) }); } }
        quote! { #ident: { let token = tokens.get(#path).ok_or_else(|| ::dtcg_runtime::DtcgError::Projection { path: #path.into(), message: "required token is missing".into() })?; registry.convert::<#ty>(token)? } }
    });
    let component_impl =
        component.then(|| quote! { impl ::dtcg_runtime::DtcgComponentTokens for #name {} });
    quote! { impl ::dtcg_runtime::DtcgTokens for #name { fn project(tokens: &::dtcg_runtime::ResolvedTokens, registry: &::dtcg_runtime::TypeRegistry) -> Result<Self, ::dtcg_runtime::DtcgError> { Ok(Self { #(#fields),* }) } } #component_impl }.into()
}
