/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use quote::ToTokens;
use syn::{Attribute, Meta};
//use syn::{spanned::Spanned, Attribute, Error, Meta, NestedMeta, Path};

pub fn contains_skip(attrs: &[Attribute]) -> bool {
    for attr in attrs.iter() {
        if let Ok(Meta::Path(path)) = attr.parse_meta() {
            if path.to_token_stream().to_string().as_str() == "skip_serialize" {
                return true
            }
        }
    }
    false
}

/*
pub fn contains_initialize_with(attrs: &[Attribute]) -> syn::Result<Option<Path>> {
    for attr in attrs.iter() {
        if let Ok(Meta::List(meta_list)) = attr.parse_meta() {
            if meta_list.path.to_token_stream().to_string().as_str() == "init_serialize" {
                if meta_list.nested.len() != 1 {
                    return Err(Error::new(
                        meta_list.span(),
                        "init_serialize requires exactly one initialization method.",
                    ))
                }
                let nested_meta = meta_list.nested.iter().next().unwrap();
                if let NestedMeta::Meta(Meta::Path(path)) = nested_meta {
                    return Ok(Some(path.clone()))
                }
            }
        }
    }
    Ok(None)
}
*/
