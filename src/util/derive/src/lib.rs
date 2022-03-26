extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::Span;
use proc_macro_crate::crate_name;
use syn::{Ident, ItemStruct};

use darkfi_derive_internal::{struct_de, struct_ser};

#[proc_macro_derive(SerialEncodable)]
pub fn darkfi_serialize(input: TokenStream) -> TokenStream {
    let cratename = Ident::new(
        &crate_name("darkfi").unwrap_or_else(|_| "crate".to_string()),
        Span::call_site(),
    );

    let res = if let Ok(input) = syn::parse::<ItemStruct>(input) {
        struct_ser(&input, cratename)
    } else {
        // For now we only allow derive on structs
        unreachable!()
    };

    TokenStream::from(match res {
        Ok(res) => res,
        Err(err) => err.to_compile_error(),
    })
}

#[proc_macro_derive(SerialDecodable)]
pub fn darkfi_deserialize(input: TokenStream) -> TokenStream {
    let cratename = Ident::new(
        &crate_name("darkfi").unwrap_or_else(|_| "crate".to_string()),
        Span::call_site(),
    );

    let res = if let Ok(input) = syn::parse::<ItemStruct>(input) {
        struct_de(&input, cratename)
    } else {
        // For now we only allow derive on structs
        unreachable!()
    };

    TokenStream::from(match res {
        Ok(res) => res,
        Err(err) => err.to_compile_error(),
    })
}
