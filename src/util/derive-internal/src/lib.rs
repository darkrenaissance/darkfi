//! Derive (de)serialization for structs, see src/util/derive
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Fields, Ident, Index, ItemStruct, WhereClause};

pub fn struct_ser(input: &ItemStruct, cratename: Ident) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let (_impl_generics, _ty_generics, where_clause) = input.generics.split_for_impl();
    let mut where_clause = where_clause.map_or_else(
        || WhereClause { where_token: Default::default(), predicates: Default::default() },
        Clone::clone,
    );

    let mut body = TokenStream2::new();
    match &input.fields {
        Fields::Named(fields) => {
            let ln = quote! {
                let mut len = 0;
            };
            body.extend(ln);

            for field in &fields.named {
                if let Some(attr) = field.attrs.iter().next() {
                    if attr.path.is_ident("skip_serialize") {
                        continue
                    }
                }

                let field_name = field.ident.as_ref().unwrap();
                let delta = quote! {
                    len += self.#field_name.encode(&mut s)?;
                };
                body.extend(delta);

                let field_type = &field.ty;
                where_clause.predicates.push(
                    syn::parse2(quote! {
                        #field_type: #cratename::util::serial::Encodable
                    })
                    .unwrap(),
                );
            }

            let ret = quote! {
                Ok(len)
            };
            body.extend(ret)
        }
        Fields::Unnamed(fields) => {
            let ln = quote! {
                let mut len = 0;
            };
            body.extend(ln);

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

            let ret = quote! {
                Ok(len)
            };
            body.extend(ret)
        }
        Fields::Unit => {}
    }

    Ok(quote! {
        impl #cratename::util::serial::Encodable for #name #where_clause {
            fn encode<S: std::io::Write>(&self, mut s: S) -> #cratename::Result<usize> {
                #body
            }
        }
    })
}

pub fn struct_de(input: &ItemStruct, cratename: Ident) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let (_impl_generics, _ty_generics, where_clause) = input.generics.split_for_impl();
    let mut where_clause = where_clause.map_or_else(
        || WhereClause { where_token: Default::default(), predicates: Default::default() },
        Clone::clone,
    );

    let return_value = match &input.fields {
        Fields::Named(fields) => {
            let mut body = TokenStream2::new();

            for field in &fields.named {
                let field_name = field.ident.as_ref().unwrap();
                let delta: TokenStream2;
                let attr = field.attrs.iter().next();

                if attr.is_some() && attr.unwrap().path.is_ident("skip_serialize") {
                    delta = {
                        let field_type = &field.ty;
                        where_clause.predicates.push(
                            syn::parse2(quote! {
                                #field_type: core::default::Default
                            })
                            .unwrap(),
                        );

                        quote! {
                            #field_name: #field_type::default(),
                        }
                    };
                } else {
                    delta = {
                        let field_type = &field.ty;
                        where_clause.predicates.push(
                            syn::parse2(quote! {
                                #field_type: #cratename::util::serial::Decodable
                            })
                            .unwrap(),
                        );

                        quote! {
                            #field_name: #cratename::util::serial::Decodable::decode(&mut d)?,
                        }
                    };
                }

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
                    #cratename::util::serial::Decodable::decode(&mut d)?,
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
        impl #cratename::util::serial::Decodable for #name #where_clause {
            fn decode<D: std::io::Read>(mut d: D) -> #cratename::Result<Self> {
                Ok(#return_value)
            }
        }
    })
}
