//! Derive macros for `lencode` encoding/decoding traits.
//!
//! - `#[derive(Encode)]` implements `lencode::Encode` by writing fields in declaration order
//!   and encoding enum discriminants compactly.
//! - `#[derive(Decode)]` implements `lencode::Decode` to read the same layout.
//! - `#[derive(Pack)]` implements `lencode::pack::Pack` by packing/unpacking fields in
//!   declaration order. For `#[repr(transparent)]` single‑field structs, it additionally
//!   generates bulk `pack_slice`/`unpack_vec` overrides that transmute to/from the inner
//!   type's slice/vec, enabling zero‑copy bulk I/O for newtypes over byte arrays.
//!
//! For C‑like enums with an explicit `#[repr(uN/iN)]`, the numeric value of the discriminant
//! is preserved; otherwise, the variant index is used.
use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Attribute, DeriveInput, Ident, Result, Type, parse_quote, parse2};

/// Returns `true` if `#[repr(transparent)]` is present on the item.
fn has_repr_transparent(attrs: &[Attribute]) -> bool {
    for attr in attrs {
        if attr.path().is_ident("repr") {
            let mut found = false;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("transparent") {
                    found = true;
                }
                Ok(())
            });
            if found {
                return true;
            }
        }
    }
    false
}

fn enum_repr_ty(attrs: &[Attribute]) -> Option<Type> {
    let mut out: Option<Type> = None;
    for attr in attrs {
        if attr.path().is_ident("repr") {
            let _ = attr.parse_nested_meta(|meta| {
                if let Some(ident) = meta.path.get_ident() {
                    match ident.to_string().as_str() {
                        "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64"
                        | "isize" => {
                            let ty_ident = Ident::new(&ident.to_string(), Span::call_site());
                            out = Some(parse_quote!(#ty_ident));
                        }
                        _ => {}
                    }
                }
                Ok(())
            });
        }
    }
    out
}

fn crate_path() -> TokenStream2 {
    // Resolve the path to the main `lencode` crate from the macro crate, honoring any
    // potential crate renames by the downstream user. In ambiguous contexts like doctests,
    // prefer the absolute `::lencode` path.
    let found = crate_name("lencode");
    match found {
        Ok(FoundCrate::Itself) => quote!(::lencode),
        Ok(FoundCrate::Name(actual_name)) => {
            let ident = Ident::new(&actual_name, Span::call_site());
            quote!(::#ident)
        }
        Err(_) => quote!(::lencode),
    }
}

/// Derives `lencode::Encode` for structs and enums.
///
/// - Structs: fields are encoded in declaration order.
/// - Enums: a compact discriminant is written, then any fields as for structs. C‑like enums
///   with `#[repr(uN/iN)]` preserve the numeric discriminant.
#[proc_macro_derive(Encode)]
pub fn derive_encode(input: TokenStream) -> TokenStream {
    match derive_encode_impl(input) {
        Ok(ts) => ts.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Derives `lencode::Decode` for structs and enums.
///
/// The layout matches what `#[derive(Encode)]` produces.
#[proc_macro_derive(Decode)]
pub fn derive_decode(input: TokenStream) -> TokenStream {
    match derive_decode_impl(input) {
        Ok(ts) => ts.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Derives `lencode::pack::Pack` for structs.
///
/// - Fields are packed/unpacked in declaration order using their own `Pack` impls.
/// - For `#[repr(transparent)]` single‑field structs, bulk `pack_slice` and `unpack_vec`
///   overrides are generated that transmute to/from the inner type's slice/vec, enabling
///   zero‑copy bulk I/O for newtypes over byte arrays.
///
/// # Example
///
/// ```ignore
/// #[repr(transparent)]
/// #[derive(Pack)]
/// struct MyPubkey([u8; 32]);
/// ```
#[proc_macro_derive(Pack)]
pub fn derive_pack(input: TokenStream) -> TokenStream {
    match derive_pack_impl(input) {
        Ok(ts) => ts.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[inline(always)]
fn derive_encode_impl(input: impl Into<TokenStream2>) -> Result<TokenStream2> {
    let derive_input = parse2::<DeriveInput>(input.into())?;
    let krate = crate_path();
    let name = derive_input.ident.clone();
    // Prepare generics and add Encode bounds for all type parameters
    let mut generics = derive_input.generics.clone();
    {
        // Collect type parameter idents first to avoid borrow conflicts
        let type_idents: Vec<Ident> = generics.type_params().map(|tp| tp.ident.clone()).collect();
        let where_clause = generics.make_where_clause();
        for ident in type_idents {
            // Add `T: Encode` bound for each type parameter `T`
            where_clause
                .predicates
                .push(parse_quote!(#ident: #krate::prelude::Encode));
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    match derive_input.data {
        syn::Data::Struct(data_struct) => {
            let fields = data_struct.fields;
            let encode_body = match fields {
                syn::Fields::Named(ref named_fields) => {
                    let field_encodes = named_fields.named.iter().map(|f| {
                        let fname = &f.ident;
                        let ftype = &f.ty;
                        quote! {
                            total_bytes += <#ftype as #krate::prelude::Encode>::encode_ext(&self.#fname, writer, dedupe_encoder.as_deref_mut())?;
                        }
                    });
                    quote! {
                        #(#field_encodes)*
                    }
                }
                syn::Fields::Unnamed(ref unnamed_fields) => {
                    let field_encodes = unnamed_fields.unnamed.iter().enumerate().map(|(i, f)| {
                        let index = syn::Index::from(i);
                        let ftype = &f.ty;
                        quote! {
                            total_bytes += <#ftype as #krate::prelude::Encode>::encode_ext(&self.#index, writer, dedupe_encoder.as_deref_mut())?;
                        }
                    });
                    quote! {
                        #(#field_encodes)*
                    }
                }
                syn::Fields::Unit => quote! {},
            };
            Ok(quote! {
                impl #impl_generics #krate::prelude::Encode for #name #ty_generics #where_clause {
                    #[inline(always)]
                    fn encode_ext(
                        &self,
                        writer: &mut impl #krate::io::Write,
                        mut dedupe_encoder: Option<&mut #krate::dedupe::DedupeEncoder>,
                    ) -> #krate::Result<usize> {
                        let mut total_bytes = 0;
                        #encode_body
                        Ok(total_bytes)
                    }
                }
            })
        }
        syn::Data::Enum(data_enum) => {
            let is_c_like = data_enum
                .variants
                .iter()
                .all(|v| matches!(v.fields, syn::Fields::Unit));
            let repr_ty = enum_repr_ty(&derive_input.attrs);
            let use_numeric_disc = is_c_like && repr_ty.is_some();
            let repr_ty_ts = repr_ty.unwrap_or(parse_quote!(usize));
            let variant_matches = data_enum.variants.iter().enumerate().map(|(idx, v)| {
				let vname = &v.ident;
				let idx_lit = syn::Index::from(idx);
				match &v.fields {
					syn::Fields::Named(named_fields) => {
						let fields: Vec<_> = named_fields
							.named
							.iter()
							.map(|f| (f.ident.as_ref().unwrap().clone(), f.ty.clone()))
							.collect();

						let field_names: Vec<_> = fields.iter().map(|(ident, _)| ident).collect();
						let field_encodes = fields.iter().map(|(fname, ftype)| {
							quote! {
								total_bytes += <#ftype as #krate::prelude::Encode>::encode_ext(#fname, writer, dedupe_encoder.as_deref_mut())?;
							}
						});
						quote! {
							#name::#vname { #(#field_names),* } => {
								total_bytes += <usize as #krate::prelude::Encode>::encode_discriminant(#idx_lit as usize, writer)?;
								#(#field_encodes)*
							}
						}
					}
					syn::Fields::Unnamed(unnamed_fields) => {
						let fields: Vec<_> = unnamed_fields
							.unnamed
							.iter()
							.enumerate()
							.map(|(i, f)| (Ident::new(&format!("field{}", i), Span::call_site()), f.ty.clone()))
							.collect();

						let field_indices: Vec<_> = fields.iter().map(|(ident, _)| ident).collect();
						let field_encodes = fields.iter().map(|(fname, ftype)| {
							quote! {
								total_bytes += <#ftype as #krate::prelude::Encode>::encode_ext(#fname, writer, dedupe_encoder.as_deref_mut())?;
							}
						});
						quote! {
							#name::#vname( #(#field_indices),* ) => {
								total_bytes += <usize as #krate::prelude::Encode>::encode_discriminant(#idx_lit as usize, writer)?;
								#(#field_encodes)*
							}
						}
					}
					syn::Fields::Unit => {
                        if use_numeric_disc {
                            quote! {
                                #name::#vname => {
                                    let disc = (#name::#vname as #repr_ty_ts) as usize;
                                    total_bytes += <usize as #krate::prelude::Encode>::encode_discriminant(disc, writer)?;
                                }
                            }
                        } else {
                            quote! {
                                #name::#vname => {
                                    total_bytes += <usize as #krate::prelude::Encode>::encode_discriminant(#idx_lit as usize, writer)?;
                                }
                            }
                        }
                    }
				}
			});
            Ok(quote! {
                impl #impl_generics #krate::prelude::Encode for #name #ty_generics #where_clause {
                    #[inline(always)]
                    fn encode_ext(
                        &self,
                        writer: &mut impl #krate::io::Write,
                        mut dedupe_encoder: Option<&mut #krate::dedupe::DedupeEncoder>,
                    ) -> #krate::Result<usize> {
                        let mut total_bytes = 0;
                        match self {
                            #(#variant_matches)*
                        }
                        Ok(total_bytes)
                    }
                }
            })
        }
        syn::Data::Union(_data_union) => {
            // Unions are not supported
            Err(syn::Error::new_spanned(
                derive_input.ident,
                "Encode cannot be derived for unions",
            ))
        }
    }
}

#[inline(always)]
fn derive_decode_impl(input: impl Into<TokenStream2>) -> Result<TokenStream2> {
    let derive_input = parse2::<DeriveInput>(input.into())?;
    let krate = crate_path();
    let name = derive_input.ident.clone();
    // Prepare generics and add Decode bounds for all type parameters
    let mut generics = derive_input.generics.clone();
    {
        // Collect type parameter idents first to avoid borrow conflicts
        let type_idents: Vec<Ident> = generics.type_params().map(|tp| tp.ident.clone()).collect();
        let where_clause = generics.make_where_clause();
        for ident in type_idents {
            // Add `T: Decode` bound for each type parameter `T`
            where_clause
                .predicates
                .push(parse_quote!(#ident: #krate::prelude::Decode));
        }
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    match derive_input.data {
        syn::Data::Struct(data_struct) => {
            let fields = data_struct.fields;
            let decode_body = match fields {
                syn::Fields::Named(ref named_fields) => {
                    let field_decodes = named_fields.named.iter().map(|f| {
                        let fname = &f.ident;
                        let ftype = &f.ty;
                        quote! {
                            #fname: <#ftype as #krate::prelude::Decode>::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
                        }
                    });
                    quote! {
                        Ok(#name {
                            #(#field_decodes)*
                        })
                    }
                }
                syn::Fields::Unnamed(ref unnamed_fields) => {
                    let field_decodes = unnamed_fields.unnamed.iter().map(|f| {
                        let ftype = &f.ty;
                        quote! {
                            <#ftype as #krate::prelude::Decode>::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
                        }
                    });
                    quote! {
                        Ok(#name(
                            #(#field_decodes)*
                        ))
                    }
                }
                syn::Fields::Unit => quote! { Ok(#name) },
            };
            Ok(quote! {
                impl #impl_generics #krate::prelude::Decode for #name #ty_generics #where_clause {
                    #[inline(always)]
                    fn decode_ext(
                        reader: &mut impl #krate::io::Read,
                        mut dedupe_decoder: Option<&mut #krate::dedupe::DedupeDecoder>,
                    ) -> #krate::Result<Self> {
                        #decode_body
                    }
                }
            })
        }
        syn::Data::Enum(data_enum) => {
            let is_c_like = data_enum
                .variants
                .iter()
                .all(|v| matches!(v.fields, syn::Fields::Unit));
            let repr_ty = enum_repr_ty(&derive_input.attrs);
            let use_numeric_disc = is_c_like && repr_ty.is_some();
            let repr_ty_ts = repr_ty.unwrap_or(parse_quote!(usize));
            let variant_matches = data_enum.variants.iter().enumerate().map(|(idx, v)| {
                let vname = &v.ident;
                let idx_lit = syn::Index::from(idx);
                match &v.fields {
                    syn::Fields::Named(named_fields) => {
                        let field_decodes = named_fields.named.iter().map(|f| {
                            let fname = &f.ident;
                            let ftype = &f.ty;
							quote! {
								#fname: <#ftype as #krate::prelude::Decode>::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
							}
						});
                        quote! {
                            #idx_lit => Ok(#name::#vname { #(#field_decodes)* }),
                        }
                    }
                    syn::Fields::Unnamed(unnamed_fields) => {
                        let field_decodes = unnamed_fields.unnamed.iter().map(|f| {
                            let ftype = &f.ty;
                            quote! {
                                <#ftype as #krate::prelude::Decode>::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
                            }
                        });
                        quote! {
                            #idx_lit => Ok(#name::#vname( #(#field_decodes)* )),
                        }
                    }
                    syn::Fields::Unit => {
                        if use_numeric_disc {
                            quote! {
                                disc if disc == ((#name::#vname as #repr_ty_ts) as usize) => Ok(#name::#vname),
                            }
                        } else {
                            quote! {
                                #idx_lit => Ok(#name::#vname),
                            }
                        }
                    }
                }
            });
            Ok(quote! {
                impl #impl_generics #krate::prelude::Decode for #name #ty_generics #where_clause {
                    #[inline(always)]
                    fn decode_ext(
                        reader: &mut impl #krate::io::Read,
                        mut dedupe_decoder: Option<&mut #krate::dedupe::DedupeDecoder>,
                    ) -> #krate::Result<Self> {
                        let variant_idx = <usize as #krate::prelude::Decode>::decode_discriminant(reader)?;
                        match variant_idx {
                            #(#variant_matches)*
                            _ => Err(#krate::io::Error::InvalidData),
                        }
                    }
                }
            })
        }
        syn::Data::Union(_data_union) => {
            // Unions are not supported
            Err(syn::Error::new_spanned(
                derive_input.ident,
                "Decode cannot be derived for unions",
            ))
        }
    }
}

#[inline(always)]
fn derive_pack_impl(input: impl Into<TokenStream2>) -> Result<TokenStream2> {
    let derive_input = parse2::<DeriveInput>(input.into())?;
    let krate = crate_path();
    let name = derive_input.ident.clone();

    let data_struct = match derive_input.data {
        syn::Data::Struct(s) => s,
        _ => {
            return Err(syn::Error::new_spanned(
                name,
                "Pack can only be derived for structs",
            ));
        }
    };

    let is_transparent = has_repr_transparent(&derive_input.attrs);

    // Collect fields info
    let fields = &data_struct.fields;
    let field_count = fields.len();

    let (pack_body, unpack_body) = match fields {
        syn::Fields::Named(named) => {
            let pack_stmts = named.named.iter().map(|f| {
                let fname = &f.ident;
                let ftype = &f.ty;
                quote! {
                    total += <#ftype as #krate::pack::Pack>::pack(&self.#fname, writer)?;
                }
            });
            let unpack_fields = named.named.iter().map(|f| {
                let fname = &f.ident;
                let ftype = &f.ty;
                quote! {
                    #fname: <#ftype as #krate::pack::Pack>::unpack(reader)?,
                }
            });
            (
                quote! {
                    let mut total = 0usize;
                    #(#pack_stmts)*
                    Ok(total)
                },
                quote! {
                    Ok(#name {
                        #(#unpack_fields)*
                    })
                },
            )
        }
        syn::Fields::Unnamed(unnamed) => {
            let pack_stmts = unnamed.unnamed.iter().enumerate().map(|(i, f)| {
                let index = syn::Index::from(i);
                let ftype = &f.ty;
                quote! {
                    total += <#ftype as #krate::pack::Pack>::pack(&self.#index, writer)?;
                }
            });
            let unpack_fields = unnamed.unnamed.iter().map(|f| {
                let ftype = &f.ty;
                quote! {
                    <#ftype as #krate::pack::Pack>::unpack(reader)?,
                }
            });
            (
                quote! {
                    let mut total = 0usize;
                    #(#pack_stmts)*
                    Ok(total)
                },
                quote! {
                    Ok(#name(
                        #(#unpack_fields)*
                    ))
                },
            )
        }
        syn::Fields::Unit => (quote! { Ok(0) }, quote! { Ok(#name) }),
    };

    // For #[repr(transparent)] single-field structs, generate bulk pack_slice/unpack_vec
    let bulk_methods = if is_transparent && field_count == 1 {
        let inner_ty = match fields {
            syn::Fields::Named(named) => &named.named[0].ty,
            syn::Fields::Unnamed(unnamed) => &unnamed.unnamed[0].ty,
            _ => unreachable!(),
        };
        quote! {
            #[inline(always)]
            fn pack_slice(items: &[Self], writer: &mut impl #krate::io::Write) -> #krate::Result<usize> {
                // SAFETY: #[repr(transparent)] guarantees identical layout.
                let inner: &[#inner_ty] = unsafe {
                    core::slice::from_raw_parts(
                        items.as_ptr() as *const #inner_ty,
                        items.len(),
                    )
                };
                <#inner_ty as #krate::pack::Pack>::pack_slice(inner, writer)
            }

            #[inline(always)]
            fn unpack_vec(reader: &mut impl #krate::io::Read, count: usize) -> #krate::Result<Vec<Self>> {
                let inner = <#inner_ty as #krate::pack::Pack>::unpack_vec(reader, count)?;
                // SAFETY: #[repr(transparent)] guarantees identical layout.
                Ok(unsafe { core::mem::transmute::<Vec<#inner_ty>, Vec<#name>>(inner) })
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        impl #krate::pack::Pack for #name {
            #[inline(always)]
            fn pack(&self, writer: &mut impl #krate::io::Write) -> #krate::Result<usize> {
                #pack_body
            }

            #[inline(always)]
            fn unpack(reader: &mut impl #krate::io::Read) -> #krate::Result<Self> {
                #unpack_body
            }

            #bulk_methods
        }
    })
}

#[test]
fn test_derive_encode_struct_basic() {
    let tokens = quote! {
        struct TestStruct {
            a: u32,
            b: String,
        }
    };
    let derived = derive_encode_impl(tokens).unwrap();
    let expected = quote! {
        impl ::lencode::prelude::Encode for TestStruct {
            #[inline(always)]
            fn encode_ext(
                &self,
                writer: &mut impl ::lencode::io::Write,
                mut dedupe_encoder: Option<&mut ::lencode::dedupe::DedupeEncoder>,
            ) -> ::lencode::Result<usize> {
                let mut total_bytes = 0;
                total_bytes += <u32 as ::lencode::prelude::Encode>::encode_ext(
                    &self.a,
                    writer,
                    dedupe_encoder.as_deref_mut()
                )?;
                total_bytes += <String as ::lencode::prelude::Encode>::encode_ext(
                    &self.b,
                    writer,
                    dedupe_encoder.as_deref_mut()
                )?;
                Ok(total_bytes)
            }
        }
    };
    assert_eq!(derived.to_string(), expected.to_string());
}

#[test]
fn test_derive_decode_struct_basic() {
    let tokens = quote! {
        struct TestStruct {
            a: u32,
            b: String,
        }
    };
    let derived = derive_decode_impl(tokens).unwrap();
    let expected = quote! {
        impl ::lencode::prelude::Decode for TestStruct {
            #[inline(always)]
            fn decode_ext(
                reader: &mut impl ::lencode::io::Read,
                mut dedupe_decoder: Option<&mut ::lencode::dedupe::DedupeDecoder>,
            ) -> ::lencode::Result<Self> {
                Ok(TestStruct {
                    a: <u32 as ::lencode::prelude::Decode>::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
                    b: <String as ::lencode::prelude::Decode>::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
                })
            }
        }
    };
    assert_eq!(derived.to_string(), expected.to_string());
}

#[test]
fn test_derive_pack_named_struct() {
    let tokens = quote! {
        struct Point {
            x: u32,
            y: u32,
        }
    };
    let derived = derive_pack_impl(tokens).unwrap();
    let expected = quote! {
        impl ::lencode::pack::Pack for Point {
            #[inline(always)]
            fn pack(&self, writer: &mut impl ::lencode::io::Write) -> ::lencode::Result<usize> {
                let mut total = 0usize;
                total += <u32 as ::lencode::pack::Pack>::pack(&self.x, writer)?;
                total += <u32 as ::lencode::pack::Pack>::pack(&self.y, writer)?;
                Ok(total)
            }

            #[inline(always)]
            fn unpack(reader: &mut impl ::lencode::io::Read) -> ::lencode::Result<Self> {
                Ok(Point {
                    x: <u32 as ::lencode::pack::Pack>::unpack(reader)?,
                    y: <u32 as ::lencode::pack::Pack>::unpack(reader)?,
                })
            }
        }
    };
    assert_eq!(derived.to_string(), expected.to_string());
}

#[test]
fn test_derive_pack_transparent_tuple_struct() {
    let tokens = quote! {
        #[repr(transparent)]
        struct MyKey([u8; 32]);
    };
    let derived = derive_pack_impl(tokens).unwrap();
    // Just verify it parses and contains key signatures; exact whitespace around >> varies.
    let s = derived.to_string();
    assert!(s.contains("pack_slice"), "should contain pack_slice override");
    assert!(s.contains("unpack_vec"), "should contain unpack_vec override");
    assert!(s.contains("transmute"), "should contain transmute for bulk decode");
    assert!(s.contains("from_raw_parts"), "should contain from_raw_parts for bulk encode");
}
