use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{DeriveInput, Ident, Result, parse_quote, parse2};

fn crate_path() -> TokenStream2 {
    // Resolve the path to the main `lencode` crate from the macro crate,
    // honoring any potential crate renames by the downstream user.
    // Fallback to `crate` during unit tests or when used inside `lencode` itself.
    let found = crate_name("lencode");
    match found {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(actual_name)) => {
            let ident = Ident::new(&actual_name, Span::call_site());
            quote!(::#ident)
        }
        Err(_) => quote!(crate),
    }
}

#[proc_macro_derive(Encode)]
pub fn derive_encode(input: TokenStream) -> TokenStream {
    match derive_encode_impl(input) {
        Ok(ts) => ts.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro_derive(Decode)]
pub fn derive_decode(input: TokenStream) -> TokenStream {
    match derive_decode_impl(input) {
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
						quote! {
							#name::#vname => {
								total_bytes += <usize as #krate::prelude::Encode>::encode_discriminant(#idx_lit as usize, writer)?;
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
                        quote! {
                            #idx_lit => Ok(#name::#vname),
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
        impl crate::prelude::Encode for TestStruct {
            #[inline(always)]
            fn encode_ext(
                &self,
                writer: &mut impl crate::io::Write,
                mut dedupe_encoder: Option<&mut crate::dedupe::DedupeEncoder>,
            ) -> crate::Result<usize> {
                let mut total_bytes = 0;
                total_bytes += <u32 as crate::prelude::Encode>::encode_ext(
                    &self.a,
                    writer,
                    dedupe_encoder.as_deref_mut()
                )?;
                total_bytes += <String as crate::prelude::Encode>::encode_ext(
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
        impl crate::prelude::Decode for TestStruct {
            #[inline(always)]
            fn decode_ext(
                reader: &mut impl crate::io::Read,
                mut dedupe_decoder: Option<&mut crate::dedupe::DedupeDecoder>,
            ) -> crate::Result<Self> {
                Ok(TestStruct {
                    a: <u32 as crate::prelude::Decode>::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
                    b: <String as crate::prelude::Decode>::decode_ext(reader, dedupe_decoder.as_deref_mut())?,
                })
            }
        }
    };
    assert_eq!(derived.to_string(), expected.to_string());
}
