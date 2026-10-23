//! The input a schema reads.
//!
//! A schema has to accept data whose shape nobody has proven yet: a JSON
//! body, a form, a query string, a configuration file. [`Value`] is the one
//! shape they all reduce to, and the only place in this crate that admits it
//! does not know the type yet.
//!
//! Data the program already holds in its own types never becomes a `Value`.
//! It goes to `check` instead, which keeps the type it has.

use std::fmt;

/// Input of a type the schema has not yet confirmed.
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    /// Absent, null, or an empty form field.
    Null,
    /// A boolean.
    Bool(bool),
    /// A whole number.
    Int(i64),
    /// A decimal number.
    Real(f64),
    /// Text.
    Text(String),
    /// An ordered list.
    List(Vec<Value>),
    /// Named fields, in the order they arrived.
    Map(Vec<(String, Value)>),
}

impl Value {
    /// The name of this kind of value, for an error message.
    pub const fn kind(&self) -> &'static str {
        match self {
            Value::Null => "nothing",
            Value::Bool(_) => "a boolean",
            Value::Int(_) => "a whole number",
            Value::Real(_) => "a decimal number",
            Value::Text(_) => "text",
            Value::List(_) => "a list",
            Value::Map(_) => "an object",
        }
    }

    /// Take a field out of a map, leaving the rest. Returns `Null` when the
    /// map has no such field, because absent and null are the same answer to
    /// a schema.
    pub fn take(&mut self, name: &str) -> Value {
        match self {
            Value::Map(fields) => match fields.iter().position(|(k, _)| k == name) {
                Some(i) => fields.remove(i).1,
                None => Value::Null,
            },
            _ => Value::Null,
        }
    }

    /// The names still left in a map, for the strict object rule.
    pub fn spare_keys(&self) -> Vec<String> {
        match self {
            Value::Map(fields) => fields.iter().map(|(k, _)| k.clone()).collect(),
            _ => Vec::new(),
        }
    }

    /// Read an `application/x-www-form-urlencoded` body.
    ///
    /// Every field arrives as [`Value::Text`], because a form carries no
    /// types. A field with no value becomes [`Value::Null`]. A name that
    /// repeats becomes a [`Value::List`], in arrival order, which is what a
    /// group of checkboxes sends.
    pub fn from_form(body: &str) -> Value {
        let mut fields: Vec<(String, Value)> = Vec::new();
        for pair in body.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (raw_name, raw_value) = match pair.find('=') {
                Some(i) => (&pair[..i], Some(&pair[i + 1..])),
                None => (pair, None),
            };
            let name = unescape(raw_name);
            let value = match raw_value {
                None => Value::Null,
                Some("") => Value::Null,
                Some(text) => Value::Text(unescape(text)),
            };
            match fields.iter_mut().find(|(k, _)| *k == name) {
                Some((_, held)) => match held {
                    Value::List(items) => items.push(value),
                    other => {
                        let first = std::mem::replace(other, Value::Null);
                        *other = Value::List(vec![first, value]);
                    }
                },
                None => fields.push((name, value)),
            }
        }
        Value::Map(fields)
    }

    /// Read a query string. Same rules as a form, which is the same format.
    pub fn from_query(query: &str) -> Value {
        Value::from_form(query.strip_prefix('?').unwrap_or(query))
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind())
    }
}

/// Percent decoding, with `+` for a space. A sequence that is not valid
/// UTF-8 keeps its bytes as the replacement character, because a form is
/// untrusted input and a panic is not a validation error.
fn unescape(s: &str) -> String {
    if !s.contains('%') && !s.contains('+') {
        return s.to_string();
    }
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(hi), Some(lo)) => {
                    out.push(hi * 16 + lo);
                    i += 3;
                }
                _ => {
                    out.push(b'%');
                    i += 1;
                }
            },
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Value {
        Value::Text(v.to_string())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Value {
        Value::Text(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Value {
        Value::Bool(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Value {
        Value::Real(v)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Value {
        match v {
            Some(inner) => inner.into(),
            None => Value::Null,
        }
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Value {
        Value::List(v.into_iter().map(Into::into).collect())
    }
}

macro_rules! from_int {
    ($($t:ty),+) => {
        $(impl From<$t> for Value {
            fn from(v: $t) -> Value {
                Value::Int(v as i64)
            }
        })+
    };
}

from_int!(i8, i16, i32, i64, u8, u16, u32);

#[cfg(feature = "json")]
impl From<serde_json::Value> for Value {
    fn from(v: serde_json::Value) -> Value {
        use serde_json::Value as J;
        match v {
            J::Null => Value::Null,
            J::Bool(b) => Value::Bool(b),
            J::Number(n) => match n.as_i64() {
                Some(i) => Value::Int(i),
                None => Value::Real(n.as_f64().unwrap_or(f64::NAN)),
            },
            J::String(s) => Value::Text(s),
            J::Array(items) => Value::List(items.into_iter().map(Value::from).collect()),
            J::Object(fields) => Value::Map(
                fields
                    .into_iter()
                    .map(|(k, v)| (k, Value::from(v)))
                    .collect(),
            ),
        }
    }
}
