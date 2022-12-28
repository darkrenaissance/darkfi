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

//! Derive (de)serialization for structs, see src/serial/derive
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Fields, Ident, Index, ItemEnum, ItemStruct, WhereClause};

mod helpers;
use helpers::contains_skip;

pub fn enum_ser(input: &ItemEnum, cratename: Ident) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let mut where_clause = where_clause.map_or_else(
        || WhereClause { where_token: Default::default(), predicates: Default::default() },
        Clone::clone,
    );

    let mut variant_idx_body = TokenStream2::new();
    let mut fields_body = TokenStream2::new();
    for (variant_idx, variant) in input.variants.iter().enumerate() {
        let variant_idx = u8::try_from(variant_idx).expect("up to 256 enum variants are supported");
        let variant_ident = &variant.ident;
        let mut variant_header = TokenStream2::new();
        let mut variant_body = TokenStream2::new();
        match &variant.fields {
            Fields::Named(fields) => {
                for field in &fields.named {
                    let field_name = field.ident.as_ref().unwrap();
                    if contains_skip(&field.attrs) {
                        variant_header.extend(quote! { _ #field_name, }); // TODO: Test this
                        continue
                    } else {
                        let field_type = &field.ty;
                        where_clause.predicates.push(
                            syn::parse2(quote! {
                                #field_type: #cratename::Encodable
                            })
                            .unwrap(),
                        );
                        variant_header.extend(quote! { #field_name, });
                    }
                    variant_body.extend(quote! {
                        len += self.#field_name.encode(&mut s);
                    })
                }
                variant_header = quote! { { #variant_header } };
                variant_idx_body.extend(quote!(
                    #name::#variant_ident { .. } => #variant_idx,
                ));
            }
            Fields::Unnamed(fields) => {
                for (field_idx, field) in fields.unnamed.iter().enumerate() {
                    let field_idx =
                        u32::try_from(field_idx).expect("up to 2^32 fields are supported");
                    if contains_skip(&field.attrs) {
                        let field_ident =
                            Ident::new(format!("_id{}", field_idx).as_str(), Span::call_site());
                        variant_header.extend(quote! { #field_ident, });
                        continue
                    } else {
                        let field_type = &field.ty;
                        where_clause.predicates.push(
                            syn::parse2(quote! {
                                #field_type: #cratename::Encodable
                            })
                            .unwrap(),
                        );

                        let field_ident =
                            Ident::new(format!("id{}", field_idx).as_str(), Span::call_site());
                        variant_header.extend(quote! { #field_ident, });
                        variant_body.extend(quote! {
                            len += self.#field_ident.encode(&mut s)?;
                        })
                    }
                }
                variant_header = quote! { ( #variant_header )};
                variant_idx_body.extend(quote!(
                    #name::#variant_ident(..) => #variant_idx,
                ));
            }
            Fields::Unit => {
                variant_idx_body.extend(quote!(
                    #name::#variant_ident => #variant_idx,
                ));
            }
        }
        fields_body.extend(quote!(
            #name::#variant_ident #variant_header => {
                #variant_body
            }
        ))
    }

    Ok(quote! {
        impl #impl_generics #cratename::Encodable for #name #ty_generics #where_clause {
            fn encode<S: std::io::Write>(&self, mut s: S) -> ::core::result::Result<usize, std::io::Error> {
                let variant_idx: u8 = match self {
                    #variant_idx_body
                };

                s.write_all(&variant_idx.to_le_bytes())?;
                let mut len = 1;

                match self {
                    #fields_body
                }

                Ok(len)
            }
        }
    })
}

pub fn enum_de(input: &ItemEnum, cratename: Ident) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let mut where_clause = where_clause.map_or_else(
        || WhereClause { where_token: Default::default(), predicates: Default::default() },
        Clone::clone,
    );

    let mut variant_arms = TokenStream2::new();
    for (variant_idx, variant) in input.variants.iter().enumerate() {
        let variant_idx = u8::try_from(variant_idx).expect("up to 256 enum variants are supported");
        let variant_ident = &variant.ident;
        let mut variant_header = TokenStream2::new();
        match &variant.fields {
            Fields::Named(fields) => {
                for field in &fields.named {
                    let field_name = field.ident.as_ref().unwrap();
                    if contains_skip(&field.attrs) {
                        variant_header.extend(quote! {
                                #field_name: Default::default(),
                        });
                    } else {
                        let field_type = &field.ty;
                        where_clause.predicates.push(
                            syn::parse2(quote! {
                                #field_type: #cratename::Decodable
                            })
                            .unwrap(),
                        );

                        variant_header.extend(quote! {
                            #field_name: #cratename::Decodable::decode(&mut d)?,
                        });
                    }
                }
                variant_header = quote! { { #variant_header } };
            }
            Fields::Unnamed(fields) => {
                for field in fields.unnamed.iter() {
                    if contains_skip(&field.attrs) {
                        variant_header.extend(quote! { Default::default(), });
                    } else {
                        let field_type = &field.ty;
                        where_clause.predicates.push(
                            syn::parse2(quote! {
                                #field_type: #cratename::Decodable
                            })
                            .unwrap(),
                        );

                        variant_header.extend(quote! { #cratename::Decodable::decode(&mut d)?, });
                    }
                }
                variant_header = quote! { ( #variant_header ) };
            }
            Fields::Unit => {}
        }

        variant_arms.extend(quote! {
            #variant_idx => #name::#variant_ident #variant_header ,
        });
    }

    let variant_idx = quote! {
        let variant_idx: u8 = #cratename::Decodable::decode(&mut d)?;
    };

    Ok(quote! {
        impl #impl_generics #cratename::Decodable for #name #ty_generics #where_clause {
            fn decode<D: std::io::Read>(mut d: D) -> ::core::result::Result<Self, std::io::Error> {
                #variant_idx

                let return_value = match variant_idx {
                    #variant_arms
                    _ => {
                        let msg = format!("Unexpected variant index: {:?}", variant_idx);
                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, msg));
                    }
                };
                Ok(return_value)
            }
        }
    })
}

pub fn struct_ser(input: &ItemStruct, cratename: Ident) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let mut where_clause = where_clause.map_or_else(
        || WhereClause { where_token: Default::default(), predicates: Default::default() },
        Clone::clone,
    );

    let mut body = TokenStream2::new();

    match &input.fields {
        Fields::Named(fields) => {
            for field in &fields.named {
                if contains_skip(&field.attrs) {
                    continue
                }

                let field_name = field.ident.as_ref().unwrap();
                let delta = quote! {
                    len += self.#field_name.encode(&mut s)?;
                };
                body.extend(delta);

                let field_type = &field.ty;
                where_clause.predicates.push(
                    syn::parse2(quote! {
                        #field_type: #cratename::Encodable
                    })
                    .unwrap(),
                );
            }
        }
        Fields::Unnamed(fields) => {
            for field_idx in 0..fields.unnamed.len() {
                let field_idx = Index {
                    index: u32::try_from(field_idx).expect("up to 2^32 fields are supported"),
                    span: Span::call_site(),
                };
                let delta = quote! {
                    len += self.#field_idx.encode(&mut s)?;
                };
                body.extend(delta);
            }
        }
        Fields::Unit => {}
    }

    Ok(quote! {
        impl #impl_generics #cratename::Encodable for #name #ty_generics #where_clause {
            fn encode<S: std::io::Write>(&self, mut s: S) -> ::core::result::Result<usize, std::io::Error> {
                let mut len = 0;
                #body
                Ok(len)
            }
        }
    })
}

pub fn struct_de(input: &ItemStruct, cratename: Ident) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let mut where_clause = where_clause.map_or_else(
        || WhereClause { where_token: Default::default(), predicates: Default::default() },
        Clone::clone,
    );

    let return_value = match &input.fields {
        Fields::Named(fields) => {
            let mut body = TokenStream2::new();
            for field in &fields.named {
                let field_name = field.ident.as_ref().unwrap();

                let delta: TokenStream2 = if contains_skip(&field.attrs) {
                    quote! {
                        #field_name: Default::default(),
                    }
                } else {
                    let field_type = &field.ty;
                    where_clause.predicates.push(
                        syn::parse2(quote! {
                            #field_type: #cratename::Decodable
                        })
                        .unwrap(),
                    );

                    quote! {
                        #field_name: #cratename::Decodable::decode(&mut d)?,
                    }
                };
                body.extend(delta);
            }
            quote! {
                Self { #body }
            }
        }
        Fields::Unnamed(fields) => {
            let mut body = TokenStream2::new();
            for _ in 0..fields.unnamed.len() {
                let delta = quote! {
                    #cratename::Decodable::decode(&mut d)?,
                };
                body.extend(delta);
            }
            quote! {
                Self( #body )
            }
        }
        Fields::Unit => {
            quote! {
                Self {}
            }
        }
    };

    Ok(quote! {
        impl #impl_generics #cratename::Decodable for #name #ty_generics #where_clause {
            fn decode<D: std::io::Read>(mut d: D) -> ::core::result::Result<Self, std::io::Error> {
                Ok(#return_value)
            }
        }
    })
}
