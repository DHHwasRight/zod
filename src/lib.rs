//! Schemas a literal must satisfy while the crate compiles, and input must
//! satisfy while it runs.
//!
//! TypeScript needs Zod because its types are gone before the program runs.
//! Rust keeps them, so a port of Zod would spend most of its effort on a
//! problem `serde` already solved. What Rust does not have is the other half:
//! a rule narrower than a type, written once, and applied both to the
//! literals in the source and to the bytes off the wire.
//!
//! That is what this crate is.
//!
//! ```
//! use zod::{Int, Text};
//!
//! const HANDLE: Text = Text::new().min(3).max(16).slug();
//! const PORT: Int = Int::new().from(1).to(65535);
//!
//! // The compiler runs the rule. Change "ada-lovelace" to "a" and this
//! // file stops compiling.
//! zod::accepts!(HANDLE, "ada-lovelace");
//! zod::accepts!(PORT, 8080);
//!
//! // The same rule, against something only known at run time.
//! assert!(HANDLE.faults("ada-lovelace").is_empty());
//! assert_eq!(HANDLE.faults("A").len(), 2); // too short, and not a slug
//! ```
//!
//! # The two questions
//!
//! A schema answers two questions that libraries usually confuse:
//!
//! - **parse**: is this unknown input a valid `T`? It converts, and it can
//!   fail on the type before it reaches a rule.
//! - **check**: is this `T`, which the caller already holds, one the rules
//!   accept? No conversion, every fault reported, and the type already
//!   proven by the compiler.
//!
//! `check` is the one a typed language needs most, and the one a Zod port
//! leaves out. A caller that built the value itself has already settled the
//! first question; re-parsing it throws away the proof.

#![deny(missing_docs)]

pub mod flaw;
pub mod number;
pub mod schema;
pub mod text;
pub mod value;

pub use flaw::{Code, Fault, Faults, Flaw, Path, Step};
pub use number::{Int, Real};
pub use schema::{
    bool, list, object, text_bool, text_int, text_real, Bool, Fallback, FromText, List, Obj, Opt,
    Refine, Schema,
};
pub use text::{Shape, Text};
pub use value::Value;

/// Refuse to compile when a literal breaks a rule.
///
/// The schema must be a `const`, because the compiler evaluates it.
///
/// ```
/// use zod::Text;
/// const SLUG: Text = Text::new().filled().slug();
/// zod::accepts!(SLUG, "hello-world");
/// ```
///
/// ```compile_fail
/// use zod::Text;
/// const SLUG: Text = Text::new().filled().slug();
/// zod::accepts!(SLUG, "Hello World");
/// ```
#[macro_export]
macro_rules! accepts {
    ($schema:expr, $value:expr $(,)?) => {
        const _: () = $schema.must_accept($value);
    };
}

/// A literal the compiler has already checked.
///
/// It expands to the literal itself, so it costs nothing at run time.
///
/// ```
/// use zod::Text;
/// const HOST: Text = Text::new().filled().holds(".");
/// let url = format!("https://{}/", zod::lit!(HOST, "example.com"));
/// assert_eq!(url, "https://example.com/");
/// ```
#[macro_export]
macro_rules! lit {
    ($schema:expr, $value:expr $(,)?) => {{
        const _: () = $schema.must_accept($value);
        $value
    }};
}

/// A type whose values cannot exist unless the rules accept them.
///
/// `new` checks at run time and returns [`Faults`]. `lit` checks in a const
/// context, which means the compiler refuses the literal outright.
///
/// ```
/// use zod::{Int, Text};
///
/// zod::newtype!(
///     /// A public name, lowercase and hyphenated.
///     pub struct Handle(text) = Text::new().min(3).max(16).slug()
/// );
/// zod::newtype!(pub struct Port(int) = Int::new().from(1).to(65535));
///
/// const ADMIN: Handle = Handle::lit("ada-lovelace");
/// const HTTPS: Port = Port::lit(443);
///
/// assert_eq!(ADMIN.as_str(), "ada-lovelace");
/// assert_eq!(HTTPS.get(), 443);
/// assert!(Handle::new("no").is_err());
/// ```
#[macro_export]
macro_rules! newtype {
    ($(#[$meta:meta])* $vis:vis struct $name:ident(text) = $rules:expr $(;)?) => {
        $(#[$meta])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        $vis struct $name(::std::borrow::Cow<'static, str>);

        impl $name {
            /// The rules every value of this type satisfies.
            pub const RULES: $crate::Text = $rules;

            /// A literal the compiler checks. Bind it to a `const` and a bad
            /// literal stops the build.
            pub const fn lit(value: &'static str) -> $name {
                Self::RULES.must_accept(value);
                $name(::std::borrow::Cow::Borrowed(value))
            }

            /// Check a value the program found at run time.
            pub fn new(value: impl Into<String>) -> ::std::result::Result<$name, $crate::Faults> {
                let value = value.into();
                let flaws = Self::RULES.faults(&value);
                if flaws.is_empty() {
                    ::std::result::Result::Ok($name(::std::borrow::Cow::Owned(value)))
                } else {
                    ::std::result::Result::Err($crate::Faults::at_root(flaws))
                }
            }

            /// The text inside.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Give up the wrapper.
            pub fn into_string(self) -> String {
                self.0.into_owned()
            }
        }

        impl ::std::ops::Deref for $name {
            type Target = str;
            fn deref(&self) -> &str {
                &self.0
            }
        }

        impl ::std::convert::AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::Faults;
            fn from_str(s: &str) -> ::std::result::Result<$name, $crate::Faults> {
                $name::new(s)
            }
        }
    };

    ($(#[$meta:meta])* $vis:vis struct $name:ident(int) = $rules:expr $(;)?) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        $vis struct $name(i64);

        impl $name {
            /// The rules every value of this type satisfies.
            pub const RULES: $crate::Int = $rules;

            /// A literal the compiler checks.
            pub const fn lit(value: i64) -> $name {
                Self::RULES.must_accept(value as i128);
                $name(value)
            }

            /// Check a value the program found at run time.
            pub fn new(value: i64) -> ::std::result::Result<$name, $crate::Faults> {
                let flaws = Self::RULES.faults(value as i128);
                if flaws.is_empty() {
                    ::std::result::Result::Ok($name(value))
                } else {
                    ::std::result::Result::Err($crate::Faults::at_root(flaws))
                }
            }

            /// The number inside.
            pub const fn get(self) -> i64 {
                self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                ::std::fmt::Display::fmt(&self.0, f)
            }
        }
    };
}
