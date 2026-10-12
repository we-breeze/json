//! Attribute macros for typed JSON models.
use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::quote;
use syn::{ItemStruct, parse_macro_input, parse_quote};

/// Mark presence-aware fields with `#[json(presence)]`. Add `lenient` to
/// explicitly tolerate non-null values that cannot deserialize as their type.
#[proc_macro_attribute]
pub fn model(args: TokenStream, input: TokenStream) -> TokenStream {
    if !args.is_empty() {
        return syn::Error::new(proc_macro2::Span::call_site(), "model takes no arguments")
            .to_compile_error()
            .into();
    }
    let mut item = parse_macro_input!(input as ItemStruct);
    let root = match crate_name("brz-json") {
        Ok(FoundCrate::Name(name)) => {
            let name = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#name)
        }
        _ => quote!(::brz_json),
    };
    for field in &mut item.fields {
        let mut presence = false;
        let mut lenient = false;
        let mut retained = Vec::new();
        for attribute in std::mem::take(&mut field.attrs) {
            if !attribute.path().is_ident("json") {
                retained.push(attribute);
                continue;
            }
            if let Err(error) = attribute.parse_nested_meta(|meta| {
                if meta.path.is_ident("presence") && !presence {
                    presence = true;
                    Ok(())
                } else if meta.path.is_ident("lenient") && !lenient {
                    lenient = true;
                    Ok(())
                } else {
                    Err(meta.error("expected presence or lenient, without duplicates"))
                }
            }) {
                return error.to_compile_error().into();
            }
        }
        if lenient && !presence {
            return syn::Error::new_spanned(field, "lenient requires presence")
                .to_compile_error()
                .into();
        }
        field.attrs = retained;
        if presence {
            if field.ident.is_none() {
                return syn::Error::new_spanned(field, "presence requires a named field")
                    .to_compile_error()
                    .into();
            }
            let ty = &field.ty;
            field.ty = parse_quote!(#root::__private::Field<#ty, #lenient>);
            let mut has_default = false;
            let mut has_skip = false;
            let missing = syn::LitStr::new(
                &format!(
                    "{}::__private::Field::missing",
                    root.to_string().replace(' ', "")
                ),
                proc_macro2::Span::call_site(),
            );
            for attribute in &mut field.attrs {
                if attribute.path().is_ident("serde") {
                    let options = attribute.parse_args_with(
                        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                    );
                    if let Ok(mut options) = options {
                        for option in &mut options {
                            if option.path().is_ident("default") {
                                if !matches!(option, syn::Meta::Path(_)) {
                                    return syn::Error::new_spanned(option, "presence fields use the missing default; custom defaults belong in business logic")
                                        .to_compile_error().into();
                                }
                                has_default = true;
                                *option = parse_quote!(default = #missing);
                            }
                            has_skip |= option.path().is_ident("skip_serializing_if");
                        }
                        *attribute = parse_quote!(#[serde(#options)]);
                    }
                }
            }
            if !has_default {
                field.attrs.push(parse_quote!(#[serde(default = #missing)]));
            }
            if has_skip {
                return syn::Error::new_spanned(field, "presence fields omit missing values automatically; skip_serializing_if is unsupported")
                    .to_compile_error().into();
            }
            let skip = syn::LitStr::new(
                &format!(
                    "{}::__private::Field::is_missing",
                    root.to_string().replace(' ', "")
                ),
                proc_macro2::Span::call_site(),
            );
            field
                .attrs
                .push(parse_quote!(#[serde(skip_serializing_if = #skip)]));
        }
    }
    quote!(#item).into()
}
