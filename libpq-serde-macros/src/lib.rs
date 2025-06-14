use std::str::FromStr;

use proc_macro2::{self, TokenStream};
use quote::{format_ident, quote};
use syn::{DeriveInput, GenericArgument, Ident, PathArguments, Type};

// Derive macro are executed at compile time and can generate code
// base on a token stream which is produced from the code.

//----------------------------------------------------------------------------------
// Derive macro: SerdeLibpqData
//----------------------------------------------------------------------------------

//TODO: Cleanup the code once all cases are well understood and the interface is deemed
//stable.
#[derive(Debug, deluxe::ParseMetaItem)]
enum LengthEncoding {
    None,
    I16,
    I32,
    Null,
}

#[derive(Debug)]
struct ParseLengthEncodingError;

/// I really dont like this but I couldn't use LengthEncoding
/// in the tests, so I had to go with Strings.
impl FromStr for LengthEncoding {
    type Err = ParseLengthEncodingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(LengthEncoding::None),
            "i16" => Ok(LengthEncoding::I16),
            "i32" => Ok(LengthEncoding::I32),
            "null" => Ok(LengthEncoding::Null),
            _ => Err(ParseLengthEncodingError),
        }
    }
}

#[derive(Debug, deluxe::ExtractAttributes)]
#[deluxe(attributes(serde_libpq))]
struct SerdeLibpq {
    #[deluxe(default = "none".into())]
    length_encoding: String,
}

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
        let mut prepare_deserialize: Vec<proc_macro2::TokenStream> = Vec::new();
        let mut fields_deserialize: Vec<proc_macro2::TokenStream> = Vec::new();
        let mut fields_size: Vec<proc_macro2::TokenStream> = Vec::new();

        for field in s.fields.iter_mut() {
            let mut already_serialiazed = true;
            let mut already_prepared_deserialiazed = true;
            let mut already_sized = true;

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
            if let Type::Path(ref type_path) = field_type {
                if let Some(segment) = type_path.path.segments.iter().next() {
                    match deluxe::extract_attributes(field) {
                        Ok(attrs) => {
                            let attrs: SerdeLibpq = attrs;

                            match &segment.ident.to_string()[..] {
                                "Vec" | "Option" | "Bytes" => {
                                    let (se, pde, si) = match &segment.ident.to_string()[..] {
                                        "Vec" => serde_libpq_vec(&field_name, &attrs, &field_type),
                                        "Bytes" => serde_libpq_bytes(&field_name, &attrs),
                                        "Option" => {
                                            serde_libpq_option(&field_name, &attrs, &field_type)
                                        }
                                        _ => unreachable!(),
                                    };
                                    fields_serialize.push(se);
                                    prepare_deserialize.push(pde);
                                    fields_size.push(si);
                                }
                                _ => {
                                    already_serialiazed = false;
                                    already_prepared_deserialiazed = false;
                                    already_sized = false;
                                }
                            }
                        }
                        Err(e) => panic!("Couldn't extract macro attributes: {e}"),
                    }
                }
            } else {
                unreachable!("Couldn't get the Type::Path of a type");
            }

            if !already_serialiazed {
                fields_serialize.push(quote! { self.#field_name.serialize(buffer)?; });
            }
            if !already_prepared_deserialiazed {
                prepare_deserialize
                    .push(quote! { let #field_name = <#field_type>::deserialize(buffer)?; });
            }
            fields_deserialize.push(quote! { #field_name, });
            if !already_sized {
                fields_size.push(quote! { self.#field_name.byte_size() });
            }
        }

        Ok(quote! {
            impl libpq_serde_types::ByteSized for #ident {
                fn byte_size(&self) -> i32 {
                    0 #(+ #fields_size)*
                }
            }

            impl libpq_serde_types::Serialize for #ident {
                fn serialize(&self, buffer: &mut bytes::BytesMut) -> anyhow::Result<()> {
                    use bytes::BufMut;
                    #(#fields_serialize)*
                    Ok(())
                }
            }

            impl libpq_serde_types::Deserialize for #ident {
                fn deserialize(buffer: &mut bytes::Bytes) -> anyhow::Result<Self>
                where
                    Self: std::marker::Sized
                {
                    use bytes::Buf;
                    #(#prepare_deserialize)*

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

fn serde_libpq_vec(
    field_name: &Ident,
    attributes: &SerdeLibpq,
    ty: &Type,
) -> (TokenStream, TokenStream, TokenStream) {
    let path = match ty {
        Type::Path(path) => path,
        _ => unreachable!("Missing Path in Type for Vec"),
    };

    // Get the generic type of the vector
    let inner_type: &Type = match path
        .path
        .segments
        .first()
        .and_then(|seg| match &seg.arguments {
            PathArguments::AngleBracketed(args) => args.args.first(),
            _ => None,
        }) {
        Some(GenericArgument::Type(inner_type)) => inner_type,
        _ => unreachable!("Missing generic type for Vec"),
    };

    let length_encoding = LengthEncoding::from_str(&attributes.length_encoding);
    match length_encoding {
        Ok(LengthEncoding::I16) | Ok(LengthEncoding::I32) => {
            let (lencode, lencode_size): (Ident, i32) = match length_encoding {
                Ok(LengthEncoding::I16) => (format_ident!("i16"), 2),
                Ok(LengthEncoding::I32) => (format_ident!("i32"), 4),
                _ => unreachable!(),
            };

            // Serialize
            let comment =
                format!("Serialize Vec field: {field_name} with length encoding {lencode}");
            let serialize = quote! {
                #[doc = #comment]
                (self.#field_name.len() as #lencode).serialize(buffer)?;
                for elt in &self.#field_name {
                    elt.serialize(buffer)?;
                }
            };

            // Deserialize
            let comment =
                format!("Deserialize Vec field: {field_name} with length encoding {lencode}");
            let prepare_deserialize = quote! {
                #[doc = #comment]
                let mut #field_name: Vec<#inner_type>= Vec::new();
                let len = #lencode::deserialize(buffer)?;
                for _ in 0..len {
                    #field_name.push(#inner_type::deserialize(buffer)?);
                }
            };

            // ByteSized
            //NOTE:rust cannot infer the return type of byte_size()
            //it could lead to a type mismatch in the addition,
            //hence the turbo fish
            let size = quote! {
                (self.#field_name.iter().map(|e| e.byte_size()).sum::<i32>() + #lencode_size) as i32
            };

            (serialize, prepare_deserialize, size)
        }
        Ok(LengthEncoding::Null) => {
            // Serialize
            //NOTE: the record should ne 0x00 terminated but we dont do
            //it here because the only kind data we can have here is
            //String which are null terminated
            //FIXME: make sure we cannot have anything elese than a
            //String as the generic type.
            let comment = format!("Serialize Vec field: {field_name} with length encoding null");
            let serialize = quote! {
                #[doc = #comment]
                for elt in &self.#field_name {
                    elt.serialize(buffer)?;
                }
                buffer.put_u8(0x00);
            };

            // Deserialize
            let comment = format!("Deserialize Vec field: {field_name} with length encoding null");
            let prepare_deserialize = quote! {
                #[doc = #comment]
                let mut #field_name: Vec<#inner_type> = Vec::new();
                loop {
                    dbg!(buffer.len());
                    if buffer.len() == 1 {
                        if let 0 = buffer.try_get_u8()? {
                            break;
                        } else {
                            return Err(anyhow::anyhow!(
                                "Incorrect terminator null terminated vec"
                            ));
                        }
                    } else if buffer.len() == 0 {
                        return Err(anyhow::anyhow!(
                            "Missing null terminator in null terminated vec"
                        ));
                    } else {
                        #field_name.push(#inner_type::deserialize(buffer)?);
                    }
                }
            };

            // ByteSized
            let size = quote! {
                (self.#field_name.iter().map(|e| e.byte_size() as i32).sum::<i32>() + 1) as i32
            };

            (serialize, prepare_deserialize, size)
        }
        Ok(LengthEncoding::None) => panic!("length encoding = \"None\" is note supported fot Vec"),
        Err(_) => panic!("length_encoding only accepts none, i16, i32 or null"),
    }
}

fn serde_libpq_bytes(
    field_name: &Ident,
    attributes: &SerdeLibpq,
) -> (TokenStream, TokenStream, TokenStream) {
    let (lencode, lencode_size) = match LengthEncoding::from_str(&attributes.length_encoding) {
        Ok(LengthEncoding::I16) => (format_ident!("i16"), 2),
        Ok(LengthEncoding::I32) => (format_ident!("i32"), 4),
        Ok(LengthEncoding::Null) => {
            panic!("length encoding = \"Null\" is note supported fot bytes::Bytes")
        }
        Ok(LengthEncoding::None) => {
            panic!("length encoding = \"None\" is note supported fot bytes::Bytes ")
        }
        Err(_) => panic!("length_encoding only accepts none, i16, i32 or null"),
    };

    // Serialize
    let comment = format!("Serialize Bytes field: {field_name} with length encoding {lencode}");
    let serialize = quote! {
        #[doc = #comment]
        (self.#field_name.len() as #lencode).serialize(buffer)?;
        buffer.put_slice(&self.#field_name.slice(0..self.#field_name.len()));
    };

    // Deserialize
    let comment = format!("Deserialize Bytes field: {field_name} with length encoding {lencode}");
    let prepare_deserialize = quote! {
        #[doc = #comment]
        let len = #lencode::deserialize(buffer)?;
        let mut #field_name = vec![0_u8; len as usize];
        //FIXME: Can we do without the copy?
        buffer.try_copy_to_slice(&mut #field_name)?;
        let #field_name: Bytes = Bytes::from(#field_name);
    };

    // ByteSized
    let size = quote! {
        (self.#field_name.len() as i32 + #lencode_size) as i32
    };

    (serialize, prepare_deserialize, size)
}

fn serde_libpq_option(
    field_name: &Ident,
    attributes: &SerdeLibpq,
    ty: &Type,
) -> (TokenStream, TokenStream, TokenStream) {
    let path = match ty {
        Type::Path(path) => path,
        _ => unreachable!("Missing Path in Option"),
    };

    let inner_type: &Type = match path
        .path
        .segments
        .first()
        .and_then(|seg| match &seg.arguments {
            PathArguments::AngleBracketed(args) => args.args.first(),
            _ => None,
        }) {
        Some(GenericArgument::Type(inner_type)) => inner_type,
        _ => unreachable!("Missing generic type for Option"),
    };

    let inner_path = match inner_type {
        Type::Path(inner_path) => inner_path,
        _ => unreachable!("Missing Path in inner type in Option"),
    };

    let segment = match inner_path.path.segments.iter().next() {
        Some(segment) => segment,
        _ => unreachable!("Couldn't get the Type::Path of a type"),
    };

    match &segment.ident.to_string()[..] {
        //FIXME: there is a lot of duplicated code here. Mostly because in the vec and bytes
        //case we access the data with self.#field_name and not here. Let's make it work
        //first.
        "Vec" => {
            // get the generic type of the array
            let inner_type = match inner_type {
                Type::Path(path) => path,
                _ => unreachable!("Missing Path in Option"),
            };
            let vec_inner_type: &Type =
                match inner_type
                    .path
                    .segments
                    .first()
                    .and_then(|seg| match &seg.arguments {
                        PathArguments::AngleBracketed(args) => args.args.first(),
                        _ => None,
                    }) {
                    Some(GenericArgument::Type(inner_type)) => inner_type,
                    _ => unreachable!("Missing generic type for Vec"),
                };

            let (lencode, lencode_size, func_put) =
                match LengthEncoding::from_str(&attributes.length_encoding) {
                    Ok(LengthEncoding::I16) => (format_ident!("i16"), 2, format_ident!("put_i16")),
                    Ok(LengthEncoding::I32) => (format_ident!("i32"), 4, format_ident!("put_i32")),
                    _ => panic!("Option<Vec<T>> only supports length_encoding i16 or i32"),
                };

            let comment =
                format!("Serialize Option Vec field: {field_name} with length encoding {lencode}");
            let serialize = quote! {
                #[doc = #comment]
                match self.#field_name {
                    Some(ref t) => {
                        (t.len() as #lencode).serialize(buffer)?;
                        for elt in t.iter() {
                            elt.serialize(buffer)?;
                        }
                    }
                    None => buffer.#func_put(-1),
                };
            };

            let comment = format!(
                "Deserialize Option Vec field: {field_name} with length encoding {lencode}"
            );
            let prepare_deserialize = quote! {
                #[doc = #comment]
                let len = #lencode::deserialize(buffer)?;
                let #field_name = {
                    if len == -1 {
                        None
                    } else {
                        Some({
                            let mut #field_name: Vec<#vec_inner_type>= Vec::new();
                            for _ in 0..len {
                                #field_name.push(#vec_inner_type::deserialize(buffer)?);
                            }
                            #field_name
                        })
                    }
                };
            };

            let size = quote! {
                {
                    match self.#field_name {
                        None => #lencode_size,
                        Some(ref t) => (t.iter().map(|e| e.byte_size()).sum::<i32>() + #lencode_size) as i32
                    }
                }
            };

            return (serialize, prepare_deserialize, size);
        }
        "Bytes" => {
            let (lencode, lencode_size, func_put) =
                match LengthEncoding::from_str(&attributes.length_encoding) {
                    Ok(LengthEncoding::I16) => (format_ident!("i16"), 2, format_ident!("put_i16")),
                    Ok(LengthEncoding::I32) => (format_ident!("i32"), 4, format_ident!("put_i32")),
                    _ => panic!("Option<Vec<T>> only supports length_encoding i16 or i32"),
                };

            // Serialize
            let comment = format!(
                "Serialize Option Bytes field: {field_name} with length encoding {lencode}"
            );
            let serialize = quote! {
                #[doc = #comment]
                match self.#field_name {
                    Some(ref t) => {
                        (t.len() as #lencode).serialize(buffer)?;
                        buffer.put_slice(&t.slice(0..t.len()));
                    }
                    None => buffer.#func_put(-1),
                };
            };

            // Deserialize
            let comment = format!(
                "Deserialize Option Bytes field: {field_name} with length encoding {lencode}"
            );
            let prepare_deserialize = quote! {
                #[doc = #comment]
                let len = #lencode::deserialize(buffer)?;
                let #field_name = {
                    if len == -1 {
                        None
                    } else {
                        Some({
                            let mut #field_name = vec![0_u8; len as usize];
                            //FIXME: Can we do without the copy?
                            buffer.try_copy_to_slice(&mut #field_name)?;
                            Bytes::from(#field_name)
                        })
                    }
                };
            };

            // ByteSized
            let size = quote! {
                match self.#field_name {
                    None => #lencode_size,
                    Some(ref t) => {
                        (t.len() as i32 + #lencode_size) as i32
                    }
                }
            };

            return (serialize, prepare_deserialize, size);
        }
        _ => unimplemented!(
            "Option is not implemented for type {}",
            &segment.ident.to_string()
        ),
    }

    todo!()
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
            impl crate::message::MessageBody for #ident {
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
// Derive macro: TryFromRawMessage
//----------------------------------------------------------------------------------

#[proc_macro_derive(TryFromRawMessage, attributes(message_body))]
pub fn try_from_raw_message_derive_macro(
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    try_from_raw_message_derive_macro2(input.into()) // transform the stream to a procmacro2 one
        .expect("proc macro must return a TokenStream rather than a Result")
        .into() // to fo back proc_macro::TokenStream
}

fn try_from_raw_message_derive_macro2(
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
            impl TryFrom<&mut crate::message::RawMessage<crate::message::MessageType>> for #ident where #ident: Deserialize{
                type Error = anyhow::Error;

                fn try_from(message: &mut crate::message::RawMessage<crate::message::MessageType>) -> anyhow::Result<#ident> {
                    if #kind as u8 == message.mtype.main {
                        #ident::deserialize(&mut message.body)
                    } else {
                        Err(anyhow::anyhow!(
                            "Impossible to create struct from RawMessage: {:?}", message.mtype
                        ))
                    }
                }
            }
        })
    } else {
        panic!("An unsupported type was given for TryFromRawMessage (supported: struct, enum with one field)");
    }
}
