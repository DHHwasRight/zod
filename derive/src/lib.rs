//! The `#[derive(Entry)]` macro for the `zod` crate.
//!
//! It writes the schema a struct carries, so the struct itself guards the
//! entrance: the fields hold the rules, the container holds the refusal,
//! and no call site has to remember either.

use proc_macro::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{parse_macro_input, Data, DeriveInput, Expr, Fields, LitStr, Path, Type};

/// Make a struct read itself out of unknown input.
///
/// On the struct:
///
/// - `#[entry(strict)]` refuses a field the struct does not declare.
/// - `#[entry(refuse = Type)]` names what a refused entrance gives back.
///   The type implements `Refusal` and `Default`. Without it, the entrance
///   gives back a `String`.
///
/// On a field:
///
/// - `#[check(expr)]` is the schema, any `Schema` whose output is the field
///   type. Without it, the field type's own schema reads it.
/// - `#[nested]` reads another struct that derives `Entry`.
/// - `#[name("wire_name")]` when the name on the wire differs.
/// - `#[label("Human name")]` names the field in a message.
/// - `#[says("one sentence")]` replaces every message the field produces.
#[proc_macro_derive(Entry, attributes(entry, check, nested, name, label, says))]
pub fn entry(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match build(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

struct Field {
    ident: syn::Ident,
    wire: String,
    schema: proc_macro2::TokenStream,
}

fn build(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    let mut strict = false;
    let mut refuse: Option<Path> = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("entry") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("strict") {
                strict = true;
                return Ok(());
            }
            if meta.path.is_ident("refuse") {
                refuse = Some(meta.value()?.parse()?);
                return Ok(());
            }
            Err(meta.error("expected `strict` or `refuse = Type`"))
        })?;
    }

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "Entry needs named fields; a tuple struct has no names to read",
                ))
            }
        },
        _ => return Err(syn::Error::new(
            input.span(),
            "Entry works on a struct; an enum needs a union, which this crate does not have yet",
        )),
    };

    let mut parts = Vec::new();
    for field in fields {
        parts.push(one(field)?);
    }

    let reads = parts.iter().map(|f| {
        let ident = &f.ident;
        let wire = &f.wire;
        let schema = &f.schema;
        quote! {
            let #ident = {
                let schema = #schema;
                let found = ::zod::Value::take(&mut input, #wire);
                walk.field(#wire, |walk| ::zod::Schema::read(&schema, found, walk))
            };
        }
    });

    let takes = parts.iter().map(|f| {
        let ident = &f.ident;
        quote! { #ident: #ident? }
    });

    let looks = parts.iter().map(|f| {
        let ident = &f.ident;
        let wire = &f.wire;
        let schema = &f.schema;
        quote! {
            {
                let schema = #schema;
                walk.field(#wire, |walk| ::zod::Schema::inspect(&schema, &self.#ident, walk));
            }
        }
    });

    let spare = if strict {
        quote! { ::zod::schema::refuse_spare(&input, walk); }
    } else {
        quote! {}
    };

    let deny = match refuse {
        Some(path) => quote! { #path },
        None => quote! { ::zod::Summary },
    };

    Ok(quote! {
        impl #impl_generics ::zod::Entry for #name #type_generics #where_clause {
            type Deny = #deny;

            fn take(input: ::zod::Value, walk: &mut ::zod::Walk) -> ::core::option::Option<Self> {
                let mut input = ::zod::schema::open_map(input, walk)?;
                let before = walk.found();
                #(#reads)*
                #spare
                if walk.found() != before {
                    return ::core::option::Option::None;
                }
                ::core::option::Option::Some(Self { #(#takes),* })
            }

            fn look(&self, walk: &mut ::zod::Walk) {
                #(#looks)*
            }
        }

        impl #impl_generics ::zod::Native for #name #type_generics #where_clause {
            type By = ::zod::Nested<Self>;

            fn by() -> ::zod::Nested<Self> {
                ::zod::nested()
            }
        }
    })
}

fn one(field: &syn::Field) -> syn::Result<Field> {
    let ident = field.ident.clone().expect("named fields only");
    let mut wire = ident.to_string();
    let mut check: Option<Expr> = None;
    let mut is_nested = false;
    let mut label: Option<LitStr> = None;
    let mut says: Option<LitStr> = None;

    for attr in &field.attrs {
        let path = attr.path();
        if path.is_ident("check") {
            check = Some(attr.parse_args()?);
        } else if path.is_ident("nested") {
            is_nested = true;
        } else if path.is_ident("name") {
            wire = attr.parse_args::<LitStr>()?.value();
        } else if path.is_ident("label") {
            label = Some(attr.parse_args()?);
        } else if path.is_ident("says") {
            says = Some(attr.parse_args()?);
        }
    }

    if check.is_some() && is_nested {
        return Err(syn::Error::new(
            field.span(),
            "a field takes `check` or `nested`, not both: a nested struct carries its own rules",
        ));
    }

    let ty: &Type = &field.ty;
    let mut schema = match (&check, is_nested) {
        (Some(expr), _) => quote! { #expr },
        (None, true) => quote! { ::zod::nested::<#ty>() },
        (None, false) => quote! { <#ty as ::zod::Native>::by() },
    };

    if let Some(says) = says {
        schema = quote! { ::zod::Schema::says(#schema, #says) };
    }
    if let Some(label) = label {
        schema = quote! { ::zod::Schema::label(#schema, #label) };
    }

    Ok(Field {
        ident,
        wire,
        schema,
    })
}
