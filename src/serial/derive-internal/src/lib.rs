/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Derive (de)serialization for enums and structs, see src/serial/derive
use std::collections::HashMap;

use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::{punctuated::Punctuated, token::Comma, Attribute, Path, Variant, WherePredicate};

mod sync_derive;
pub use sync_derive::{enum_de, enum_ser, struct_de, struct_ser};

#[cfg(feature = "async")]
mod async_derive;
#[cfg(feature = "async")]
pub use async_derive::{async_enum_de, async_enum_ser, async_struct_de, async_struct_ser};

struct VariantParts {
    where_predicates: Vec<WherePredicate>,
    variant_header: TokenStream,
    variant_body: TokenStream,
    variant_idx_body: TokenStream,
}

/// Calculates the discriminant that will be assigned by the compiler.
/// See: <https://doc.rust-lang.org/reference/items/enumerations.html#assigning-discriminant-values>
fn discriminant_map(variants: &Punctuated<Variant, Comma>) -> HashMap<Ident, TokenStream> {
    let mut map = HashMap::new();

    let mut next_discriminant_if_not_specified = quote! {0};

    for variant in variants {
        let this_discriminant = variant
            .discriminant
            .clone()
            .map_or_else(|| quote! { #next_discriminant_if_not_specified }, |(_, e)| quote! { #e });

        next_discriminant_if_not_specified = quote! { #this_discriminant + 1 };
        map.insert(variant.ident.clone(), this_discriminant);
    }

    map
}

pub fn contains_skip(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("skip_serialize"))
}

pub fn contains_initialize_with(attrs: &[Attribute]) -> Option<Path> {
    for attr in attrs.iter() {
        if attr.path().is_ident("init_serialize") {
            let mut res = None;
            let _ = attr.parse_nested_meta(|meta| {
                res = Some(meta.path);
                Ok(())
            });
            return res
        }
    }

    None
}
