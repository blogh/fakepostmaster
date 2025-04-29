use proc_macro;
use proc_macro2;
use quote::quote;
use syn::DeriveInput;

//----------------------------------------------------------------------------------
// Derive macro: SerdeLibpqData
//----------------------------------------------------------------------------------

//#[derive(Debug, deluxe::ParseMetaItem)]
//enum Transform {
//    Vec32,
//    Vec16,
//    VecNull,
//    None,
//}
//
//#[derive(Debug, deluxe::ExtractAttributes)]
//#[deluxe(attributes(serde_libpq))]
//struct SerdeLibpq {
//    #[deluxe(default = Transform::None)]
//    transform: Transform,
//}

#[proc_macro_derive(SerdeLibpqData, attributes(serde_libpq))]
/// Implements the Serialize and ByteSized traits on a struct.
pub fn serde_libpq_data_derive_macro(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    serde_libpq_data_derive_macro2(input.into()) // transform the stream to a procmacro2 one
        .expect("proc macro must return a TokenStream rather than a Result")
        .into() // to fo back proc_macro::TokenStream
}

fn serde_libpq_data_derive_macro2(
    input: proc_macro2::TokenStream,
) -> deluxe::Result<proc_macro2::TokenStream> {
    // parse
    let mut ast: DeriveInput = syn::parse2(input)?;

    if let syn::Data::Struct(s) = &mut ast.data {
        // define impl variables
        let ident = &ast.ident;
        //let (impl_generics, type_generics, where_clause) = &ast.split_for_impl();

        // extract field attribute
        let mut fields_serialize: Vec<proc_macro2::TokenStream> = Vec::new();
        let mut fields_deserialize: Vec<proc_macro2::TokenStream> = Vec::new();
        let mut fields_size: Vec<proc_macro2::TokenStream> = Vec::new();

        for field in s.fields.iter_mut() {
            //NOTE: can we avoid the clone here ? (deluxe::extract_attributes(field))
            // takes a mutable borrow
            let field_name = field
                .ident
                .as_ref()
                .expect("Failed to access ident for filed, tuple struct are not supported")
                .clone();

            //NOTE: can we avoid the clone here ? (deluxe::extract_attributes(field))
            // takes a mutable borrow
            let field_type = field.ty.clone();

            //if let Type::Path(ref type_path) = field_type {
            //    if let Some(segment) = type_path.path.segments.iter().next() {
            //        if let Ok(attrs) = deluxe::extract_attributes(field) {
            //            let attrs: SerdeLibpq = attrs;
            //            match attrs.transform {
            //                Transform::None => {
            //                    fields_serialize
            //                        .push(quote! { self.#field_name.serialize(buffer); });
            //                    fields_deserialize.push(
            //                        quote! { #field_name: <#field_type>::deserialize(buffer)?, },
            //                    );
            //                    fields_size.push(quote! { self.#field_name.byte_size() });
            //                }
            //                Transform::Vec16 => {
            //                    //FIXME: do something along thoses lines to do a runtimecheck of the type.
            //                    //assert!(std::any::type_name::<#field_type>().contains("Vec"));
            //                    if segment.ident == "Vec" {
            //                        fields_serialize.push(
            //                            quote! { Vec16::from(self.#field_name).serialize(buffer); },
            //                        );
            //                        fields_deserialize.push(quote! { #field_name: Vec::from(<#field_type>::deserialize(buffer)?), });
            //                        fields_size.push(
            //                            quote! { Vec16::from(&self.#field_name).byte_size() },
            //                        );
            //                    } else {
            //                        panic!("Invalid target for transform=Vec16");
            //                    }
            //                }
            //                Transform::Vec32 => {
            //                    //FIXME: do something along thoses lines to do a runtimecheck of the type.
            //                    //assert!(std::any::type_name::<#field_type>().contains("Vec"));
            //                    if segment.ident == "Vec" {
            //                        fields_serialize.push(
            //                            quote! { Vec32::from(self.#field_name).serialize(buffer); },
            //                        );
            //                        fields_deserialize.push(quote! { #field_name: Vec::from(<#field_type>::deserialize(buffer)?), });
            //                        fields_size.push(
            //                            quote! { Vec32::from(&self.#field_name).byte_size() },
            //                        );
            //                    } else {
            //                        panic!("Invalid target for transform=Vec32");
            //                    }
            //                }
            //                Transform::VecNull => {
            //                    //FIXME: do something along thoses lines to do a runtimecheck of the type.
            //                    //assert!(std::any::type_name::<#field_type>().contains("Vec"));
            //                    if segment.ident == "Vec" {
            //                        fields_serialize.push(
            //                            quote! { Vec32::from(self.#field_name).serialize(buffer); },
            //                        );
            //                        fields_deserialize.push(quote! { #field_name: Vec::from(<#field_type>::deserialize(buffer)?), });
            //                        fields_size.push(quote! { self.#field_name.byte_size() });
            //                    } else {
            //                        panic!("Invalid target for transform=VecNull");
            //                    }
            //                }
            //            }
            //        }
            //    }
            //}

            fields_serialize.push(quote! { self.#field_name.serialize(buffer); });
            fields_deserialize.push(quote! { #field_name: <#field_type>::deserialize(buffer)?, });
            fields_size.push(quote! { self.#field_name.byte_size() });
        }

        Ok(quote! {
            impl ByteSized for #ident {
                fn byte_size(&self) -> i32 {
                    0 #(+ #fields_size)*
                }
            }

            impl Serialize for #ident {
                fn serialize(&self, buffer: &mut bytes::BytesMut) {
                    #(#fields_serialize)*
                }
            }

            impl Deserialize for #ident {
                fn deserialize(buffer: &mut bytes::Bytes) -> anyhow::Result<Self>
                where
                    Self: std::marker::Sized,
                    bytes::Bytes: bytes::Buf
                {
                    Ok(Self {
                        #(#fields_deserialize)*
                    })
                }
            }
        })
    } else {
        panic!("An unsupported type was given for serialize/deserialize/byte_size (supported: struct, enum with one field)");
    }
}

//----------------------------------------------------------------------------------
// Derive macro: MessageBody
//----------------------------------------------------------------------------------

#[derive(deluxe::ExtractAttributes)]
#[deluxe(attributes(message_body))]
struct MessageBody {
    kind: char,
}

#[proc_macro_derive(MessageBody, attributes(message_body))]
pub fn message_body_derive_macro(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    message_body_derive_macro2(input.into()) // transform the stream to a procmacro2 one
        .expect("proc macro must return a TokenStream rather than a Result")
        .into() // to fo back proc_macro::TokenStream
}

fn message_body_derive_macro2(
    input: proc_macro2::TokenStream,
) -> deluxe::Result<proc_macro2::TokenStream> {
    // parse
    let mut ast: DeriveInput = syn::parse2(input)?;

    // Extract the attributes!
    let MessageBody { kind } = deluxe::extract_attributes(&mut ast)?;

    if let syn::Data::Struct(_) = &mut ast.data {
        // define impl variables
        let ident = &ast.ident;

        Ok(quote! {
            impl MessageBody for #ident {
                fn message_type(&self) -> u8 {
                    #kind as u8
                }
            }
        })
    } else {
        panic!("An unsupported type was given for MessageBody (supported: struct, enum with one field)");
    }
}

//----------------------------------------------------------------------------------
// Derive macro: TryFromRawBackendMessage
//----------------------------------------------------------------------------------

#[proc_macro_derive(TryFromRawBackendMessage, attributes(message_body))]
pub fn try_from_raw_backend_message_derive_macro(
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    try_from_raw_backend_message_derive_macro2(input.into()) // transform the stream to a procmacro2 one
        .expect("proc macro must return a TokenStream rather than a Result")
        .into() // to fo back proc_macro::TokenStream
}

fn try_from_raw_backend_message_derive_macro2(
    input: proc_macro2::TokenStream,
) -> deluxe::Result<proc_macro2::TokenStream> {
    // parse
    let mut ast: DeriveInput = syn::parse2(input)?;

    // Extract the attributes!
    let MessageBody { kind } = deluxe::extract_attributes(&mut ast)?;

    if let syn::Data::Struct(_) = &mut ast.data {
        // define impl variables
        let ident = &ast.ident;

        Ok(quote! {
            impl TryFrom<&mut RawBackendMessage> for #ident {
                type Error = anyhow::Error;

                fn try_from(message: &mut RawBackendMessage) -> anyhow::Result<#ident> {
                    if #kind as u8 == message.header.message_type {
                        #ident::deserialize(&mut message.raw_body)
                    } else {
                        Err(anyhow!(
                            "Impossible to create struct from RawBackendMessage"
                        ))
                    }
                }
            }
        })
    } else {
        panic!("An unsupported type was given for TryFromRawBackendMessage (supported: struct, enum with one field)");
    }
}

//----------------------------------------------------------------------------------
// Derive macro: TryFromRawFrontendMessage
//----------------------------------------------------------------------------------

#[proc_macro_derive(TryFromRawFrontendMessage, attributes(message_body))]
pub fn try_from_raw_frontend_message_derive_macro(
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    try_from_raw_frontend_message_derive_macro2(input.into()) // transform the stream to a procmacro2 one
        .expect("proc macro must return a TokenStream rather than a Result")
        .into() // to fo back proc_macro::TokenStream
}

fn try_from_raw_frontend_message_derive_macro2(
    input: proc_macro2::TokenStream,
) -> deluxe::Result<proc_macro2::TokenStream> {
    // parse
    let mut ast: DeriveInput = syn::parse2(input)?;

    // Extract the attributes!
    let MessageBody { kind } = deluxe::extract_attributes(&mut ast)?;

    if let syn::Data::Struct(_) = &mut ast.data {
        // define impl variables
        let ident = &ast.ident;

        Ok(quote! {
            impl TryFrom<&mut RawFrontendMessage> for #ident {
                type Error = anyhow::Error;

                fn try_from(message: &mut RawFrontendMessage) -> anyhow::Result<#ident> {
                    if #kind as u8 == message.header.message_type {
                        #ident::deserialize(&mut message.raw_body)
                    } else {
                        Err(anyhow!(
                            "Impossible to create struct from RawFrontendMessage"
                        ))
                    }
                }
            }
        })
    } else {
        panic!("An unsupported type was given for TryFromRawFrontendMessage (supported: struct, enum with one field)");
    }
}
