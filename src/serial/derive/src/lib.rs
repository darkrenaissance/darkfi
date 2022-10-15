extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use syn::{Ident, ItemEnum, ItemStruct, ItemUnion};

use darkfi_derive_internal::{enum_de, enum_ser, struct_de, struct_ser};

#[proc_macro_derive(SerialEncodable, attributes(skip_serialize))]
pub fn darkfi_serialize(input: TokenStream) -> TokenStream {
    let found_crate = crate_name("darkfi-serial").expect("darkfi-serial is found in Cargo.toml");

    let found_crate = match found_crate {
        FoundCrate::Name(name) => name,
        FoundCrate::Itself => "crate".to_string(),
    };

    let cratename = Ident::new(&found_crate, Span::call_site());

    let res = if let Ok(input) = syn::parse::<ItemStruct>(input.clone()) {
        struct_ser(&input, cratename)
    } else if let Ok(input) = syn::parse::<ItemEnum>(input.clone()) {
        enum_ser(&input, cratename)
    } else if let Ok(_input) = syn::parse::<ItemUnion>(input) {
        todo!()
    } else {
        // Derive macros can only be defined on structs, enums, and unions.
        unreachable!()
    };

    TokenStream::from(match res {
        Ok(res) => res,
        Err(err) => err.to_compile_error(),
    })
}

#[proc_macro_derive(SerialDecodable, attributes(skip_serialize))]
pub fn darkfi_deserialize(input: TokenStream) -> TokenStream {
    let found_crate = crate_name("darkfi-serial").expect("darkfi-serial is found in Cargo.toml");

    let found_crate = match found_crate {
        FoundCrate::Name(name) => name,
        FoundCrate::Itself => "crate".to_string(),
    };

    let cratename = Ident::new(&found_crate, Span::call_site());

    let res = if let Ok(input) = syn::parse::<ItemStruct>(input.clone()) {
        struct_de(&input, cratename)
    } else if let Ok(input) = syn::parse::<ItemEnum>(input.clone()) {
        enum_de(&input, cratename)
    } else if let Ok(_input) = syn::parse::<ItemUnion>(input) {
        todo!()
    } else {
        // Derive macros can only be defined on structs, enums, and unions.
        unreachable!()
    };

    TokenStream::from(match res {
        Ok(res) => res,
        Err(err) => err.to_compile_error(),
    })
}
