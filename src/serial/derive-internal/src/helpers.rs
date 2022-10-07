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
