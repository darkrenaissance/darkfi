extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use syn::{Ident, Item, ItemStruct};

use darkfi_derive_internal::{enum_ser, struct_de, struct_ser};

#[proc_macro_derive(SerialEncodable, attributes(skip_serialize))]
pub fn darkfi_serialize(input: TokenStream) -> TokenStream {
    let found_crate = crate_name("darkfi").expect("darkfi is found in Cargo.toml");

    let found_crate = match found_crate {
        FoundCrate::Name(name) => name,
        FoundCrate::Itself => "crate".to_string(),
    };

    let cratename = Ident::new(&found_crate, Span::call_site());

    let res = match syn::parse(input).unwrap() {
        Item::Struct(strc) => struct_ser(&strc, cratename),
        Item::Enum(enu) => enum_ser(&enu, cratename),
        _ => todo!("Not implemented type"),
    };

    TokenStream::from(match res {
        Ok(res) => res,
        Err(err) => err.to_compile_error(),
    })
}

#[proc_macro_derive(SerialDecodable, attributes(skip_serialize))]
pub fn darkfi_deserialize(input: TokenStream) -> TokenStream {
    let found_crate = crate_name("darkfi").expect("darkfi is found in Cargo.toml");

    let found_crate = match found_crate {
        FoundCrate::Name(name) => name,
        FoundCrate::Itself => "crate".to_string(),
    };

    let cratename = Ident::new(&found_crate, Span::call_site());

    let res = if let Ok(input) = syn::parse::<ItemStruct>(input) {
        struct_de(&input, cratename)
    } else {
        // For now we only allow derive on structs
        todo!("Implement Enum and Union")
    };

    TokenStream::from(match res {
        Ok(res) => res,
        Err(err) => err.to_compile_error(),
    })
}
