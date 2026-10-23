//! A value on its way out.
//!
//! [`Schema::read`](crate::Schema::read) turns input into a struct. This is
//! the other direction: a struct into a [`Value`], which [`write`] renders
//! as JSON. Together they make a struct that derives
//! [`Entry`](crate::Entry) the whole contract of an endpoint -- what it
//! takes, what it gives, and what a document says about both.
//!
//! ```
//! use zod::{Entry, Give, Text};
//!
//! #[derive(Entry)]
//! struct Created {
//!     #[text(uuid)]
//!     id: String,
//!     handle: String,
//!     seats: i64,
//! }
//!
//! let made = Created {
//!     id: "0191a3f0-0000-7000-8000-000000000000".into(),
//!     handle: "ada-lovelace".into(),
//!     seats: 3,
//! };
//!
//! assert_eq!(
//!     zod::write(&made.give()),
//!     r#"{"id":"0191a3f0-0000-7000-8000-000000000000","handle":"ada-lovelace","seats":3}"#
//! );
//! ```
//!
//! There is no `serde` here and there is no reflection. The derive knows
//! the fields, because it read them, and a field's type says how to render
//! itself.

use crate::value::Value;

/// A type that can be written out as data.
///
/// `#[derive(Entry)]` writes the impl for a struct. The impls below cover
/// the field types that struct can hold.
pub trait Give {
    /// This value, as data.
    fn give(&self) -> Value;
}

impl Give for String {
    fn give(&self) -> Value {
        Value::Text(self.clone())
    }
}

impl Give for str {
    fn give(&self) -> Value {
        Value::Text(self.to_string())
    }
}

impl Give for bool {
    fn give(&self) -> Value {
        Value::Bool(*self)
    }
}

impl Give for f64 {
    fn give(&self) -> Value {
        Value::Real(*self)
    }
}

macro_rules! whole {
    ($($t:ty),+) => {
        $(impl Give for $t {
            fn give(&self) -> Value {
                Value::Int(*self as i64)
            }
        })+
    };
}

whole!(i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);

impl<T: Give> Give for Option<T> {
    fn give(&self) -> Value {
        match self {
            Some(inner) => inner.give(),
            None => Value::Null,
        }
    }
}

impl<T: Give> Give for Vec<T> {
    fn give(&self) -> Value {
        Value::List(self.iter().map(Give::give).collect())
    }
}

impl<T: Give + ?Sized> Give for &T {
    fn give(&self) -> Value {
        (**self).give()
    }
}

impl<T: Give + ?Sized> Give for Box<T> {
    fn give(&self) -> Value {
        (**self).give()
    }
}

/// Render a value as JSON.
///
/// Compact, with the fields in the order the struct declares them, because
/// a body a person diffs should not reorder itself between two runs. No
/// dependency: the whole of JSON's syntax is below, and pulling a crate in
/// to write six characters of punctuation is how a validation library ends
/// up with a dependency tree.
pub fn write(value: &Value) -> String {
    let mut out = String::new();
    put(value, &mut out);
    out
}

fn put(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Real(n) => {
            // JSON has no infinity and no NaN. A number that is neither is
            // not a number this can write, and `null` is what every other
            // writer puts there.
            if n.is_finite() {
                out.push_str(&format!("{n}"));
            } else {
                out.push_str("null");
            }
        }
        Value::Text(s) => quote(s, out),
        Value::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                put(item, out);
            }
            out.push(']');
        }
        Value::Map(fields) => {
            out.push('{');
            for (i, (name, held)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                quote(name, out);
                out.push(':');
                put(held, out);
            }
            out.push('}');
        }
    }
}

/// A JSON string, escaped by the rules in RFC 8259 §7.
fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fields_come_out_in_the_order_they_went_in() {
        let value = Value::Map(vec![
            ("b".into(), Value::Int(2)),
            ("a".into(), Value::Int(1)),
        ]);
        assert_eq!(write(&value), r#"{"b":2,"a":1}"#);
    }

    #[test]
    fn nothing_is_null_and_not_an_absent_field() {
        let value = Value::Map(vec![("note".into(), Value::Null)]);
        assert_eq!(write(&value), r#"{"note":null}"#);
    }

    #[test]
    fn a_string_is_escaped_by_the_rules_json_has() {
        let value = Value::Text("he said \"go\"\n\tnow\\".into());
        assert_eq!(write(&value), r#""he said \"go\"\n\tnow\\""#);
    }

    #[test]
    fn a_control_character_becomes_its_escape() {
        assert_eq!(write(&Value::Text("\u{1}".into())), r#""\u0001""#);
    }

    #[test]
    fn a_number_json_cannot_write_is_null() {
        assert_eq!(write(&Value::Real(f64::NAN)), "null");
        assert_eq!(write(&Value::Real(f64::INFINITY)), "null");
        assert_eq!(write(&Value::Real(0.5)), "0.5");
    }

    #[test]
    fn an_empty_list_and_an_empty_object_are_still_themselves() {
        assert_eq!(write(&Value::List(Vec::new())), "[]");
        assert_eq!(write(&Value::Map(Vec::new())), "{}");
    }

    #[test]
    fn a_list_of_values_gives_a_list_of_data() {
        let names = vec!["a".to_string(), "b".to_string()];
        assert_eq!(write(&names.give()), r#"["a","b"]"#);
    }

    #[test]
    fn an_option_is_the_value_or_nothing() {
        assert_eq!(write(&Some(3i64).give()), "3");
        assert_eq!(write(&None::<i64>.give()), "null");
    }

    #[cfg(feature = "json")]
    #[test]
    fn what_this_writes_is_what_a_parser_reads_back() {
        let value = Value::Map(vec![
            ("handle".into(), Value::Text("ada-lovelace".into())),
            ("age".into(), Value::Int(36)),
            ("tags".into(), Value::List(vec![Value::Text("x\"y".into())])),
            ("note".into(), Value::Null),
        ]);
        let text = write(&value);
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");

        // `serde_json` holds a map in sorted order unless it is built with
        // `preserve_order`, so the comparison sorts both rather than
        // asserting an order this crate deliberately keeps and that one
        // deliberately does not.
        let sorted = |value: Value| match value {
            Value::Map(mut fields) => {
                fields.sort_by(|a, b| a.0.cmp(&b.0));
                Value::Map(fields)
            }
            other => other,
        };
        assert_eq!(sorted(Value::from(parsed)), sorted(value));
    }
}
