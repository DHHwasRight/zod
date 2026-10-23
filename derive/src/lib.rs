//! The `#[derive(Entry)]` macro for the `zod` crate.
//!
//! It writes the schema a struct carries, so the struct itself guards the
//! entrance: the fields hold the rules, the container holds the refusal,
//! and no call site has to remember either.
//!
//! There are two ways to say what a field allows, and they produce the same
//! schema.
//!
//! ```ignore
//! #[check(Text::new().min(3).max(16).slug())]   // the rule as a value
//! #[text(min = 3, max = 16, slug)]              // the rule as an annotation
//! ```
//!
//! The first is for a rule that is shared, named or reused -- a `const` two
//! structs point at is one rule. The second is for a rule that belongs to
//! one field and is read once, which is most of them.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::{quote, ToTokens};
use syn::spanned::Spanned;
use syn::{
    parse_macro_input, Data, DeriveInput, Expr, Fields, LitFloat, LitInt, LitStr, Path, Type,
};

/// Make a struct read itself out of unknown input.
///
/// # On the struct
///
/// - `#[entry(strict)]` refuses a field the struct does not declare.
/// - `#[entry(refuse = Type)]` names what a refused entrance gives back.
///   The type implements `Refusal` and `Default`; `zod::refusal!` writes one
///   in three lines. Without it, the entrance gives back a `String`.
/// - `#[entry(rule = path, says = "..")]` attaches a whole-struct rule:
///   a `fn(&Self) -> bool` that sees every field at once, for the checks no
///   single field can make. Repeat the pair for more than one.
///
/// # On a field
///
/// - `#[check(expr)]` is the schema, any `Schema` whose output is the field
///   type. Without it, the field type's own schema reads it.
/// - `#[text(..)]` `#[int(..)]` `#[real(..)]` `#[list(..)]` build that
///   schema from an annotation instead. An `Option<T>` field is made
///   optional and a `Vec<T>` field becomes a list, both from the type as
///   written, so the annotation describes one item.
/// - `#[nested]` reads another struct that derives `Entry`.
/// - `#[name("wire_name")]` when the name on the wire differs.
/// - `#[label("Human name")]` names the field in a message.
/// - `#[says("one sentence")]` replaces every message the field produces.
///
/// ## What each annotation takes
///
/// | | |
/// |---|---|
/// | `text` | `min` `max` `len` as numbers, `starts` `ends` `holds` as text, and one of `filled` `ascii` `alnum` `digits` `hex` `lower` `upper` `slug` `email` `uuid` `date` |
/// | `int` | `min` `max` `step` as numbers, and `positive` `negative` `unsigned` |
/// | `real` | `min` `max` as decimals, and `positive` `wild` |
/// | `list` | `min` `max` as numbers, and `filled` |
#[proc_macro_derive(
    Entry,
    attributes(entry, check, text, int, real, list, nested, name, label, says)
)]
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
    schema: Tokens,
}

/// A whole-struct rule: a function that sees every field at once.
struct Rule {
    path: Path,
    says: LitStr,
}

fn build(input: DeriveInput) -> syn::Result<Tokens> {
    let name = &input.ident;
    let said = name.to_string();
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    let mut strict = false;
    let mut refuse: Option<Path> = None;
    let mut rules: Vec<Rule> = Vec::new();
    let mut pending: Option<Path> = None;

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
            if meta.path.is_ident("rule") {
                if let Some(held) = pending.take() {
                    return Err(meta.error(format!(
                        "`rule = {}` has no `says`: a rule nobody can read \
                         refuses with no reason",
                        held.to_token_stream()
                    )));
                }
                pending = Some(meta.value()?.parse()?);
                return Ok(());
            }
            if meta.path.is_ident("says") {
                let says: LitStr = meta.value()?.parse()?;
                let path = pending.take().ok_or_else(|| {
                    meta.error("`says` belongs to a `rule`; write `rule = f, says = \"..\"`")
                })?;
                rules.push(Rule { path, says });
                return Ok(());
            }
            Err(meta.error("expected `strict`, `refuse = Type`, or `rule = f, says = \"..\"`"))
        })?;
    }
    if let Some(held) = pending {
        return Err(syn::Error::new(
            held.span(),
            "this `rule` has no `says`: a rule nobody can read refuses with no reason",
        ));
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

    let described = parts.iter().map(|f| {
        let wire = &f.wire;
        let schema = &f.schema;
        quote! {
            ::zod::Field::of(#wire, ::zod::Schema::node(&#schema))
        }
    });

    let spare = if strict {
        quote! { ::zod::schema::refuse_spare(&input, walk); }
    } else {
        quote! {}
    };

    // A whole-struct rule runs after the fields, on a value that exists, so
    // it cannot see a field that failed to read. That is the right order:
    // "the two dates are the wrong way round" means nothing when one of
    // them is not a date.
    let whole: Vec<Tokens> = rules
        .iter()
        .map(|rule| {
            let path = &rule.path;
            let says = &rule.says;
            quote! {
                if !#path(self) {
                    walk.fault(::zod::Flaw::rule(::zod::Code::Custom, #says));
                }
            }
        })
        .collect();

    let after_read = if whole.is_empty() {
        quote! {}
    } else {
        quote! {
            let whole = Self { #(#takes),* };
            let before = walk.found();
            ::zod::Entry::look_whole(&whole, walk);
            if walk.found() != before {
                return ::core::option::Option::None;
            }
            return ::core::option::Option::Some(whole);
        }
    };
    let plain_take = if whole.is_empty() {
        let takes = parts.iter().map(|f| {
            let ident = &f.ident;
            quote! { #ident: #ident? }
        });
        quote! { ::core::option::Option::Some(Self { #(#takes),* }) }
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
                #after_read
                #plain_take
            }

            fn look(&self, walk: &mut ::zod::Walk) {
                #(#looks)*
                ::zod::Entry::look_whole(self, walk);
            }

            fn look_whole(&self, walk: &mut ::zod::Walk) {
                #(#whole)*
            }

            fn describe() -> ::zod::Node {
                ::zod::Node::named(
                    #said,
                    ::zod::Node::Object(::zod::Object {
                        fields: ::std::vec![#(#described),*],
                        strict: #strict,
                    }),
                )
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

/// What a field's `#[..]` lines add up to.
#[derive(Default)]
struct Said {
    check: Option<Expr>,
    nested: bool,
    bounds: Option<Bounds>,
    counts: Vec<Tokens>,
    label: Option<LitStr>,
    says: Option<LitStr>,
}

/// A `#[text(..)]`, `#[int(..)]` or `#[real(..)]` line, as builder calls.
struct Bounds {
    base: Tokens,
    calls: Vec<Tokens>,
    kind: &'static str,
}

fn one(field: &syn::Field) -> syn::Result<Field> {
    let ident = field.ident.clone().expect("named fields only");
    let mut wire = ident.to_string();
    let mut said = Said::default();

    for attr in &field.attrs {
        let path = attr.path();
        if path.is_ident("check") {
            said.check = Some(attr.parse_args()?);
        } else if path.is_ident("nested") {
            said.nested = true;
        } else if path.is_ident("name") {
            wire = attr.parse_args::<LitStr>()?.value();
        } else if path.is_ident("label") {
            said.label = Some(attr.parse_args()?);
        } else if path.is_ident("says") {
            said.says = Some(attr.parse_args()?);
        } else if path.is_ident("text") {
            put(&mut said, text_bounds(attr)?, attr)?;
        } else if path.is_ident("int") {
            put(&mut said, int_bounds(attr)?, attr)?;
        } else if path.is_ident("real") {
            put(&mut said, real_bounds(attr)?, attr)?;
        } else if path.is_ident("list") {
            said.counts = list_counts(attr)?;
        }
    }

    if said.check.is_some() && said.bounds.is_some() {
        return Err(syn::Error::new(
            field.span(),
            "a field says what it allows once: `check` is the rule as a value \
             and the annotation is the rule as an annotation, and two of them \
             cannot both be it",
        ));
    }
    if said.check.is_some() && said.nested {
        return Err(syn::Error::new(
            field.span(),
            "a field takes `check` or `nested`, not both: a nested struct carries its own rules",
        ));
    }

    let ty: &Type = &field.ty;
    let mut schema = match (&said.check, said.nested, &said.bounds) {
        (Some(expr), _, _) => quote! { #expr },
        (None, true, _) => quote! { ::zod::nested::<#ty>() },
        (None, false, Some(bounds)) => from_bounds(ty, bounds, &said.counts)?,
        (None, false, None) => wrap_counts(quote! { <#ty as ::zod::Native>::by() }, &said.counts),
    };

    if let Some(says) = said.says {
        schema = quote! { ::zod::Schema::says(#schema, #says) };
    }
    if let Some(label) = said.label {
        schema = quote! { ::zod::Schema::label(#schema, #label) };
    }

    Ok(Field {
        ident,
        wire,
        schema,
    })
}

fn put(said: &mut Said, bounds: Bounds, attr: &syn::Attribute) -> syn::Result<()> {
    if let Some(held) = &said.bounds {
        return Err(syn::Error::new(
            attr.span(),
            format!(
                "this field already says `{}(..)`; one annotation describes one field",
                held.kind
            ),
        ));
    }
    said.bounds = Some(bounds);
    Ok(())
}

/// `Option<T>` and `Vec<T>` as written, because a derive sees tokens and a
/// field called `Option` is the one case where that is enough.
fn inside<'t>(ty: &'t Type, outer: &str) -> Option<&'t Type> {
    let Type::Path(path) = ty else { return None };
    let last = path.path.segments.last()?;
    if last.ident != outer {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    args.args.iter().find_map(|arg| match arg {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    })
}

fn from_bounds(ty: &Type, bounds: &Bounds, counts: &[Tokens]) -> syn::Result<Tokens> {
    let base = &bounds.base;
    let calls = &bounds.calls;
    let built = quote! { #base #(#calls)* };

    // The annotation describes one value. The type says how many of them.
    let (optional, holding) = match inside(ty, "Option") {
        Some(inner) => (true, inner),
        None => (false, ty),
    };
    let listed = inside(holding, "Vec").is_some();

    let mut schema = widened(bounds, holding, listed, built)?;
    if listed {
        schema = wrap_counts(quote! { ::zod::list(#schema) }, counts);
    } else if !counts.is_empty() {
        return Err(syn::Error::new(
            holding.span(),
            "`list(..)` counts items, and this field holds one value",
        ));
    }
    if optional {
        schema = quote! { ::zod::Schema::optional(#schema) };
    }
    Ok(schema)
}

/// An `int` annotation on a field that is not an `i64` needs the width, or
/// the schema's output type will not be the field's.
fn widened(bounds: &Bounds, holding: &Type, listed: bool, built: Tokens) -> syn::Result<Tokens> {
    if bounds.kind != "int" {
        return Ok(built);
    }
    let item = if listed {
        inside(holding, "Vec").expect("a Vec was found above")
    } else {
        holding
    };
    let name = match item {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    };
    Ok(match name.as_deref() {
        Some("i64") | None => built,
        Some(_) => quote! { ::zod::Int::of::<#item>(#built) },
    })
}

fn wrap_counts(schema: Tokens, counts: &[Tokens]) -> Tokens {
    if counts.is_empty() {
        schema
    } else {
        quote! { #schema #(#counts)* }
    }
}

fn text_bounds(attr: &syn::Attribute) -> syn::Result<Bounds> {
    let mut calls = Vec::new();
    attr.parse_nested_meta(|meta| {
        let name = meta
            .path
            .get_ident()
            .map(ToString::to_string)
            .unwrap_or_default();
        match name.as_str() {
            "min" => {
                let n: LitInt = meta.value()?.parse()?;
                calls.push(quote!(.min(#n)));
            }
            "max" => {
                let n: LitInt = meta.value()?.parse()?;
                calls.push(quote!(.max(#n)));
            }
            "len" => {
                let n: LitInt = meta.value()?.parse()?;
                calls.push(quote!(.len(#n)));
            }
            "starts" => {
                let s: LitStr = meta.value()?.parse()?;
                calls.push(quote!(.starts(#s)));
            }
            "ends" => {
                let s: LitStr = meta.value()?.parse()?;
                calls.push(quote!(.ends(#s)));
            }
            "holds" => {
                let s: LitStr = meta.value()?.parse()?;
                calls.push(quote!(.holds(#s)));
            }
            "filled" => calls.push(quote!(.filled())),
            "ascii" | "alnum" | "digits" | "hex" | "lower" | "upper" | "slug" | "email"
            | "uuid" | "date" => {
                let call = syn::Ident::new(&name, meta.path.span());
                calls.push(quote!(.#call()));
            }
            other => {
                return Err(meta.error(format!(
                    "`{other}` is not a text rule; the rules are min, max, len, \
                     starts, ends, holds, filled, ascii, alnum, digits, hex, \
                     lower, upper, slug, email, uuid, date"
                )))
            }
        }
        Ok(())
    })?;
    Ok(Bounds {
        base: quote!(::zod::Text::new()),
        calls,
        kind: "text",
    })
}

fn int_bounds(attr: &syn::Attribute) -> syn::Result<Bounds> {
    let mut calls = Vec::new();
    attr.parse_nested_meta(|meta| {
        let name = meta
            .path
            .get_ident()
            .map(ToString::to_string)
            .unwrap_or_default();
        match name.as_str() {
            "min" => {
                let n: LitInt = meta.value()?.parse()?;
                calls.push(quote!(.from(#n)));
            }
            "max" => {
                let n: LitInt = meta.value()?.parse()?;
                calls.push(quote!(.to(#n)));
            }
            "step" => {
                let n: LitInt = meta.value()?.parse()?;
                calls.push(quote!(.step(#n)));
            }
            "positive" => calls.push(quote!(.positive())),
            "negative" => calls.push(quote!(.negative())),
            "unsigned" => calls.push(quote!(.unsigned())),
            other => {
                return Err(meta.error(format!(
                    "`{other}` is not a whole-number rule; the rules are min, \
                     max, step, positive, negative, unsigned"
                )))
            }
        }
        Ok(())
    })?;
    Ok(Bounds {
        base: quote!(::zod::Int::new()),
        calls,
        kind: "int",
    })
}

fn real_bounds(attr: &syn::Attribute) -> syn::Result<Bounds> {
    let mut calls = Vec::new();
    attr.parse_nested_meta(|meta| {
        let name = meta
            .path
            .get_ident()
            .map(ToString::to_string)
            .unwrap_or_default();
        match name.as_str() {
            "min" => {
                let n: LitFloat = meta.value()?.parse()?;
                calls.push(quote!(.from(#n)));
            }
            "max" => {
                let n: LitFloat = meta.value()?.parse()?;
                calls.push(quote!(.to(#n)));
            }
            "positive" => calls.push(quote!(.positive())),
            "wild" => calls.push(quote!(.wild())),
            other => {
                return Err(meta.error(format!(
                    "`{other}` is not a decimal rule; the rules are min, max, \
                     positive, wild"
                )))
            }
        }
        Ok(())
    })?;
    Ok(Bounds {
        base: quote!(::zod::Real::new()),
        calls,
        kind: "real",
    })
}

fn list_counts(attr: &syn::Attribute) -> syn::Result<Vec<Tokens>> {
    let mut calls = Vec::new();
    attr.parse_nested_meta(|meta| {
        let name = meta
            .path
            .get_ident()
            .map(ToString::to_string)
            .unwrap_or_default();
        match name.as_str() {
            "min" => {
                let n: LitInt = meta.value()?.parse()?;
                calls.push(quote!(.min(#n)));
            }
            "max" => {
                let n: LitInt = meta.value()?.parse()?;
                calls.push(quote!(.max(#n)));
            }
            "filled" => calls.push(quote!(.filled())),
            other => {
                return Err(meta.error(format!(
                    "`{other}` is not a list rule; the rules are min, max, filled"
                )))
            }
        }
        Ok(())
    })?;
    Ok(calls)
}
