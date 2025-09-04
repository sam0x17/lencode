use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{DeriveInput, Ident, Result, parse2};

fn crate_path() -> TokenStream2 {
    match crate_name(env!("CARGO_PKG_NAME")).expect("proc_macro_crate failed") {
        FoundCrate::Itself => quote!(crate),
        FoundCrate::Name(actual_name) => {
            let ident = Ident::new(&actual_name, Span::call_site());
            quote!(::#ident)
        }
    }
}

#[proc_macro_derive(Encode)]
pub fn derive_encode(input: TokenStream) -> TokenStream {
    match derive_encode_impl(input) {
        Ok(ts) => ts.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[inline(always)]
fn derive_encode_impl(input: impl Into<TokenStream2>) -> Result<TokenStream2> {
    let derive_input = parse2::<DeriveInput>(input.into())?;
    let krate = crate_path();
    match derive_input.data {
        syn::Data::Struct(data_struct) => {
            let name = derive_input.ident;
            let fields = data_struct.fields;
            let encode_body = match fields {
                syn::Fields::Named(ref named_fields) => {
                    let field_encodes = named_fields.named.iter().map(|f| {
                        let fname = &f.ident;
                        quote! {
                            total_bytes += self.#fname.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
                        }
                    });
                    quote! {
                        #(#field_encodes)*
                    }
                }
                syn::Fields::Unnamed(ref unnamed_fields) => {
                    let field_encodes = unnamed_fields.unnamed.iter().enumerate().map(|(i, _)| {
                        let index = syn::Index::from(i);
                        quote! {
                            total_bytes += self.#index.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
                        }
                    });
                    quote! {
                        #(#field_encodes)*
                    }
                }
                syn::Fields::Unit => quote! {},
            };
            return Ok(quote! {
                {
                    use #krate::prelude::*;
                    impl #krate::prelude::Encode for #name {

                        #[inline(always)]
                        fn encode_ext(
                            &self,
                            writer: &mut impl Write,
                            mut dedupe_encoder: Option<&mut DedupeEncoder>,
                        ) -> Result<usize> {
                            let mut total_bytes = 0;
                            #encode_body
                            Ok(total_bytes)
                        }
                    }
                }
            });
        }
        syn::Data::Enum(data_enum) => {
            let name = derive_input.ident;
            let variant_matches = data_enum.variants.iter().enumerate().map(|(idx, v)| {
				let vname = &v.ident;
				let idx_lit = syn::Index::from(idx);
				match &v.fields {
					syn::Fields::Named(named_fields) => {
						let field_names: Vec<_> = named_fields.named.iter().map(|f| f.ident.as_ref().unwrap()).collect();
						let field_encodes = field_names.iter().map(|fname| {
							quote! {
								total_bytes += #fname.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
							}
						});
						quote! {
							#name::#vname { #(#field_names),* } => {
								total_bytes += (#idx_lit as u64).encode_ext(writer, dedupe_encoder.as_deref_mut())?;
								#(#field_encodes)*
							}
						}
					}
					syn::Fields::Unnamed(unnamed_fields) => {
						let field_indices: Vec<syn::Ident> = (0..unnamed_fields.unnamed.len())
							.map(|i| Ident::new(&format!("field{}", i), Span::call_site()))
							.collect();
						let field_encodes = field_indices.iter().map(|fname| {
							quote! {
								total_bytes += #fname.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
							}
						});
						quote! {
							#name::#vname( #(#field_indices),* ) => {
								total_bytes += (#idx_lit as u64).encode_ext(writer, dedupe_encoder.as_deref_mut())?;
								#(#field_encodes)*
							}
						}
					}
					syn::Fields::Unit => {
						quote! {
							#name::#vname => {
								total_bytes += (#idx_lit as u64).encode_ext(writer, dedupe_encoder.as_deref_mut())?;
							}
						}
					}
				}
			});
            return Ok(quote! {
                {
                    use #krate::prelude::*;
                    impl #krate::prelude::Encode for #name {

                        #[inline(always)]
                        fn encode_ext(
                            &self,
                            writer: &mut impl Write,
                            mut dedupe_encoder: Option<&mut DedupeEncoder>,
                        ) -> Result<usize> {
                            let mut total_bytes = 0;
                            match self {
                                #(#variant_matches)*
                            }
                            Ok(total_bytes)
                        }
                    }
                }
            });
        }
        syn::Data::Union(_data_union) => {
            // Unions are not supported
            return Err(syn::Error::new_spanned(
                derive_input.ident,
                "Encode cannot be derived for unions",
            ));
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
    let expected = quote! { {
        use crate::prelude::*;
        impl crate::prelude::Encode for TestStruct {
            #[inline(always)]
            fn encode_ext(
                &self,
                writer: &mut impl Write,
                mut dedupe_encoder: Option<&mut DedupeEncoder>,
            ) -> Result<usize> {
                let mut total_bytes = 0;
                total_bytes += self.a.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
                total_bytes += self.b.encode_ext(writer, dedupe_encoder.as_deref_mut())?;
                Ok(total_bytes)
            }
        }
    } };
    assert_eq!(derived.to_string(), expected.to_string());
}
