extern crate proc_macro;
use ::core::iter::FromIterator;
use ::proc_macro::TokenStream;
use ::proc_macro2::Span;
use ::quote::quote;
use ::syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote, Data, DeriveInput, Error, Expr, Ident, LitInt, LitStr, Meta,
    Result,
};

macro_rules! die {
    ($span:expr=>
        $msg:expr
    ) => (
        return Err(Error::new($span, $msg));
    );

    (
        $msg:expr
    ) => (
        die!(Span::call_site() => $msg)
    );
}

fn literal(i: u64) -> Expr {
    let literal = LitInt::new(&i.to_string(), Span::call_site());
    parse_quote! {
        #literal
    }
}

struct EnumInfo {
    name: Ident,
    repr: Ident,
    value_expressions_to_enum_keys: Vec<(Expr, Ident)>,
}

impl Parse for EnumInfo {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok({
            let input: DeriveInput = input.parse()?;
            let name = input.ident;
            let data = if let Data::Enum(data) = input.data {
                data
            } else {
                let span = match input.data {
                    Data::Union(data) => data.union_token.span,
                    Data::Struct(data) => data.struct_token.span,
                    _ => unreachable!(),
                };
                die!(span => "Expected enum");
            };

            let repr: Ident = {
                let mut attrs = input.attrs.into_iter();
                loop {
                    if let Some(attr) = attrs.next() {
                        if let Ok(Meta::List(meta_list)) = attr.parse_meta() {
                            if let Some(ident) = meta_list.path.get_ident() {
                                if ident == "repr" {
                                    let mut nested = meta_list.nested.iter();
                                    if nested.len() != 1 {
                                        die!(ident.span()=>
                                            "Expected exactly one `repr` argument"
                                        );
                                    }
                                    let repr = nested.next().unwrap();
                                    let repr: Ident = parse_quote! {
                                        #repr
                                    };
                                    if repr == "C" {
                                        die!(repr.span()=>
                                            "repr(C) doesn't have a well defined size"
                                        );
                                    } else {
                                        break repr;
                                    }
                                }
                            }
                        }
                    } else {
                        die!("Missing `#[repr({Integer})]` attribute");
                    }
                }
            };

            let mut next_discriminant = literal(0);
            let value_expressions_to_enum_keys =
                Vec::from_iter(data.variants.into_iter().map(|variant| {
                    let disc = if let Some(d) = variant.discriminant {
                        d.1
                    } else {
                        next_discriminant.clone()
                    };
                    let variant_ident = &variant.ident;
                    next_discriminant = parse_quote! {
                        #repr::wrapping_add(#variant_ident, 1)
                    };
                    (disc, variant.ident)
                }));

            EnumInfo {
                name,
                repr,
                value_expressions_to_enum_keys,
            }
        })
    }
}

/// Implements `Into<Primitive>` for a `#[repr(Primitive)] enum`.
///
/// (It actually implements `From<Enum> for Primitive`)
///
/// ## Allows turning an enum into a primitive.
#[proc_macro_derive(IntoPrimitive)]
pub fn derive_into_primitive(input: TokenStream) -> TokenStream {
    let EnumInfo { name, repr, .. } = parse_macro_input!(input as EnumInfo);

    TokenStream::from(quote! {
        impl From<#name> for #repr {
            #[inline]
            fn from (enum_value: #name) -> Self
            {
                enum_value as Self
            }
        }
    })
}

/// Implements `TryFrom<Primitive>` for a `#[repr(Primitive)] enum`.
///
/// Attempting to turn a primitive into an enum with try_from.
#[proc_macro_derive(TryFromPrimitive)]
pub fn derive_try_from_primitive(input: TokenStream) -> TokenStream {
    let EnumInfo {
        name,
        repr,
        value_expressions_to_enum_keys,
    } = parse_macro_input!(input);

    let (match_const_exprs, enum_keys): (Vec<Expr>, Vec<Ident>) =
        value_expressions_to_enum_keys.into_iter().unzip();

    TokenStream::from(quote! {
        impl ::num_enum::TryFromPrimitive for #name {
            type Primitive = #repr;

            fn try_from_primitive (
                number: Self::Primitive,
            ) -> ::core::result::Result<
                Self, ()
            >
            {
                // Use intermediate const(s) so that enums defined like
                // `Two = ONE + 1u8` work properly.
                #![allow(non_upper_case_globals)]
                #(
                    const #enum_keys: #repr =
                        #match_const_exprs
                    ;
                )*
                match number {
                    #(
                        | #enum_keys => ::core::result::Result::Ok(
                            #name::#enum_keys
                        ),
                    )*
                    | _ => ::core::result::Result::Err(()),
                }
            }
        }

        impl ::core::convert::TryFrom<#repr> for #name {
            type Error = ();

            #[inline]
            fn try_from (
                number: #repr,
            ) -> ::core::result::Result<Self, ()>
            {
                ::num_enum::TryFromPrimitive::try_from_primitive(number)
            }
        }
    })
}

/// Generates a `unsafe fn from_unchecked (number: Primitive) -> Self`
/// associated function.
///
/// Allows unsafely turning a primitive into an enum.
/// Creating enum with invalid discriminants is undefined behavior.
#[proc_macro_derive(UnsafeFromPrimitive)]
pub fn derive_unsafe_from_primitive(stream: TokenStream) -> TokenStream {
    let EnumInfo { name, repr, .. } = parse_macro_input!(stream as EnumInfo);

    let doc_string = LitStr::new(
        &format!(
            r#"1
Transmutes `number: {repr}` into a [`{name}`].

# Safety

  - `number` must represent a valid discriminant of [`{name}`]
"#,
            repr = repr,
            name = name,
        ),
        Span::call_site(),
    );

    TokenStream::from(quote! {
        impl #name {
            #[doc = #doc_string]
            #[inline]
            pub
            unsafe
            fn from_unchecked(number: #repr) -> Self {
                ::core::mem::transmute(number)
            }
        }
    })
}
