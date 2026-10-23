//! Schemas that read unknown input, and check values the program already
//! holds.
//!
//! Every schema answers the two questions separately:
//!
//! - [`Schema::parse`] takes a [`Value`] whose type nobody has proven and
//!   gives back a Rust value, or every reason it could not.
//! - [`Schema::check`] takes the Rust value itself and runs only the rules.
//!   It cannot fail on the type, because the compiler already settled that.
//!
//! The invariant that holds them together: `check(v).is_empty()` agrees with
//! whether `parse` would accept the input that produces `v`. Neither answer
//! can drift from the other, because both call the same rules.

use crate::flaw::{Code, Fault, Faults, Flaw, Path, Step};
use crate::number::{Int, Real};
use crate::text::Text;
use crate::value::Value;

/// A rule set that reads one kind of value.
pub trait Schema {
    /// What a successful parse gives back.
    type Out;

    /// Read unknown input at a place inside a larger value.
    ///
    /// It returns `None` only when it has recorded at least one fault.
    fn read(&self, input: Value, at: &mut Path, faults: &mut Faults) -> Option<Self::Out>;

    /// Run the rules against a value that already has the right type.
    fn inspect(&self, value: &Self::Out, at: &mut Path, faults: &mut Faults);

    /// Read unknown input, from the top.
    fn parse(&self, input: impl Into<Value>) -> Result<Self::Out, Faults>
    where
        Self: Sized,
    {
        let mut at = Path::root();
        let mut faults = Faults::none();
        match self.read(input.into(), &mut at, &mut faults) {
            Some(out) if faults.is_empty() => Ok(out),
            _ => Err(faults),
        }
    }

    /// Run the rules against a value the program already holds.
    fn check(&self, value: &Self::Out) -> Faults
    where
        Self: Sized,
    {
        let mut at = Path::root();
        let mut faults = Faults::none();
        self.inspect(value, &mut at, &mut faults);
        faults
    }

    /// True when the value breaks no rule.
    fn accepts(&self, value: &Self::Out) -> bool
    where
        Self: Sized,
    {
        self.check(value).is_empty()
    }

    /// Add a rule of your own.
    fn refine(self, says: &'static str, test: fn(&Self::Out) -> bool) -> Refine<Self>
    where
        Self: Sized,
    {
        Refine {
            inner: self,
            says,
            test,
        }
    }

    /// Accept nothing as well, as `None`.
    fn optional(self) -> Opt<Self>
    where
        Self: Sized,
    {
        Opt { inner: self }
    }

    /// Put this value in when the input is nothing.
    ///
    /// The fallback goes through the rules like any other value, so a
    /// default that the schema would refuse is a fault, not a surprise.
    fn or(self, value: Self::Out) -> Fallback<Self>
    where
        Self: Sized,
    {
        Fallback { inner: self, value }
    }
}

fn wrong_type(expected: &'static str, got: &Value, at: &Path, faults: &mut Faults) {
    let _ = got;
    faults.add(Flaw::rule(Code::WrongType, expected), at);
}

impl Schema for Text {
    type Out = String;

    fn read(&self, input: Value, at: &mut Path, faults: &mut Faults) -> Option<String> {
        match input {
            Value::Text(s) => {
                let flaws = self.faults(&s);
                if flaws.is_empty() {
                    Some(s)
                } else {
                    for flaw in flaws {
                        faults.add(flaw, at);
                    }
                    None
                }
            }
            other => {
                wrong_type("text was expected", &other, at, faults);
                None
            }
        }
    }

    fn inspect(&self, value: &String, at: &mut Path, faults: &mut Faults) {
        for flaw in self.faults(value) {
            faults.add(flaw, at);
        }
    }
}

impl Schema for Int {
    type Out = i64;

    fn read(&self, input: Value, at: &mut Path, faults: &mut Faults) -> Option<i64> {
        match input {
            Value::Int(v) => {
                let flaws = self.faults(v as i128);
                if flaws.is_empty() {
                    Some(v)
                } else {
                    for flaw in flaws {
                        faults.add(flaw, at);
                    }
                    None
                }
            }
            other => {
                wrong_type("a whole number was expected", &other, at, faults);
                None
            }
        }
    }

    fn inspect(&self, value: &i64, at: &mut Path, faults: &mut Faults) {
        for flaw in self.faults(*value as i128) {
            faults.add(flaw, at);
        }
    }
}

impl Schema for Real {
    type Out = f64;

    fn read(&self, input: Value, at: &mut Path, faults: &mut Faults) -> Option<f64> {
        let v = match input {
            Value::Real(v) => v,
            Value::Int(v) => v as f64,
            other => {
                wrong_type("a number was expected", &other, at, faults);
                return None;
            }
        };
        let flaws = self.faults(v);
        if flaws.is_empty() {
            Some(v)
        } else {
            for flaw in flaws {
                faults.add(flaw, at);
            }
            None
        }
    }

    fn inspect(&self, value: &f64, at: &mut Path, faults: &mut Faults) {
        for flaw in self.faults(*value) {
            faults.add(flaw, at);
        }
    }
}

/// A boolean, which has no rule but its type.
#[derive(Clone, Copy, Default, Debug)]
pub struct Bool;

/// A boolean.
pub const fn bool() -> Bool {
    Bool
}

impl Schema for Bool {
    type Out = bool;

    fn read(&self, input: Value, at: &mut Path, faults: &mut Faults) -> Option<bool> {
        match input {
            Value::Bool(b) => Some(b),
            other => {
                wrong_type("a boolean was expected", &other, at, faults);
                None
            }
        }
    }

    fn inspect(&self, _value: &bool, _at: &mut Path, _faults: &mut Faults) {}
}

/// A schema that also accepts nothing.
#[derive(Clone, Copy, Debug)]
pub struct Opt<S> {
    inner: S,
}

impl<S: Schema> Schema for Opt<S> {
    type Out = Option<S::Out>;

    fn read(&self, input: Value, at: &mut Path, faults: &mut Faults) -> Option<Option<S::Out>> {
        match input {
            Value::Null => Some(None),
            other => self.inner.read(other, at, faults).map(Some),
        }
    }

    fn inspect(&self, value: &Option<S::Out>, at: &mut Path, faults: &mut Faults) {
        if let Some(inner) = value {
            self.inner.inspect(inner, at, faults);
        }
    }
}

/// A schema with a value to use when the input is nothing.
#[derive(Clone, Copy, Debug)]
pub struct Fallback<S: Schema> {
    inner: S,
    value: S::Out,
}

impl<S> Schema for Fallback<S>
where
    S: Schema,
    S::Out: Clone,
{
    type Out = S::Out;

    fn read(&self, input: Value, at: &mut Path, faults: &mut Faults) -> Option<S::Out> {
        match input {
            Value::Null => {
                let spare = self.value.clone();
                let before = faults.len();
                self.inner.inspect(&spare, at, faults);
                if faults.len() == before {
                    Some(spare)
                } else {
                    None
                }
            }
            other => self.inner.read(other, at, faults),
        }
    }

    fn inspect(&self, value: &S::Out, at: &mut Path, faults: &mut Faults) {
        self.inner.inspect(value, at, faults);
    }
}

/// A schema with a rule the caller wrote.
#[derive(Clone, Copy, Debug)]
pub struct Refine<S: Schema> {
    inner: S,
    says: &'static str,
    test: fn(&S::Out) -> bool,
}

impl<S: Schema> Schema for Refine<S> {
    type Out = S::Out;

    fn read(&self, input: Value, at: &mut Path, faults: &mut Faults) -> Option<S::Out> {
        let out = self.inner.read(input, at, faults)?;
        if (self.test)(&out) {
            Some(out)
        } else {
            faults.add(Flaw::rule(Code::Custom, self.says), at);
            None
        }
    }

    fn inspect(&self, value: &S::Out, at: &mut Path, faults: &mut Faults) {
        self.inner.inspect(value, at, faults);
        if !(self.test)(value) {
            faults.add(Flaw::rule(Code::Custom, self.says), at);
        }
    }
}

/// A list, and how many items it may hold.
#[derive(Clone, Copy, Debug)]
pub struct List<S> {
    item: S,
    min: usize,
    max: usize,
}

/// A list of values, each one read by the same schema.
pub const fn list<S>(item: S) -> List<S> {
    List {
        item,
        min: 0,
        max: usize::MAX,
    }
}

impl<S> List<S> {
    /// At least `n` items.
    pub const fn min(mut self, n: usize) -> List<S> {
        self.min = n;
        self
    }

    /// At most `n` items.
    pub const fn max(mut self, n: usize) -> List<S> {
        self.max = n;
        self
    }

    /// At least one item.
    pub const fn filled(self) -> List<S> {
        self.min(1)
    }

    fn count(&self, n: usize, at: &Path, faults: &mut Faults) {
        if n < self.min {
            faults.add(
                Flaw::bound(Code::TooSmall, "too few items", self.min as i128, n as i128),
                at,
            );
        }
        if n > self.max {
            faults.add(
                Flaw::bound(Code::TooBig, "too many items", self.max as i128, n as i128),
                at,
            );
        }
    }
}

impl<S: Schema> Schema for List<S> {
    type Out = Vec<S::Out>;

    fn read(&self, input: Value, at: &mut Path, faults: &mut Faults) -> Option<Vec<S::Out>> {
        let items = match input {
            Value::List(items) => items,
            other => {
                wrong_type("a list was expected", &other, at, faults);
                return None;
            }
        };
        let before = faults.len();
        self.count(items.len(), at, faults);
        let mut out = Vec::with_capacity(items.len());
        for (i, item) in items.into_iter().enumerate() {
            at.push(Step::At(i));
            if let Some(value) = self.item.read(item, at, faults) {
                out.push(value);
            }
            at.pop();
        }
        if faults.len() == before {
            Some(out)
        } else {
            None
        }
    }

    fn inspect(&self, value: &Vec<S::Out>, at: &mut Path, faults: &mut Faults) {
        self.count(value.len(), at, faults);
        for (i, item) in value.iter().enumerate() {
            at.push(Step::At(i));
            self.item.inspect(item, at, faults);
            at.pop();
        }
    }
}

/// Named fields, read into a tuple.
///
/// The fields are a tuple of `(name, schema)` pairs, and the output is a
/// tuple of their outputs, in the same order. The type comes out of the
/// schema with no derive and no macro, which is the part TypeScript needs
/// `z.infer` for.
#[derive(Clone, Copy, Debug)]
pub struct Obj<T> {
    fields: T,
    strict: bool,
}

/// An object with these fields. Unknown fields are dropped.
pub const fn object<T>(fields: T) -> Obj<T> {
    Obj {
        fields,
        strict: false,
    }
}

impl<T> Obj<T> {
    /// Refuse a field the schema does not declare.
    pub const fn strict(mut self) -> Obj<T> {
        self.strict = true;
        self
    }
}

macro_rules! objects {
    ($( [$($t:ident $v:ident $i:tt),+] )+) => {
        $(
            impl<$($t: Schema),+> Schema for Obj<($((&'static str, $t),)+)> {
                type Out = ($($t::Out,)+);

                fn read(&self, input: Value, at: &mut Path, faults: &mut Faults) -> Option<Self::Out> {
                    let mut input = match input {
                        Value::Map(fields) => Value::Map(fields),
                        other => {
                            wrong_type("an object was expected", &other, at, faults);
                            return None;
                        }
                    };
                    let before = faults.len();
                    $(
                        let $v = {
                            let (name, schema) = &self.fields.$i;
                            let field = input.take(name);
                            at.push(Step::Key((*name).to_string()));
                            let got = schema.read(field, at, faults);
                            at.pop();
                            got
                        };
                    )+
                    if self.strict {
                        for key in input.spare_keys() {
                            at.push(Step::Key(key));
                            faults.add(Flaw::rule(Code::Unexpected, "unknown field"), at);
                            at.pop();
                        }
                    }
                    if faults.len() != before {
                        return None;
                    }
                    Some(($($v?,)+))
                }

                fn inspect(&self, value: &Self::Out, at: &mut Path, faults: &mut Faults) {
                    $(
                        {
                            let (name, schema) = &self.fields.$i;
                            at.push(Step::Key((*name).to_string()));
                            schema.inspect(&value.$i, at, faults);
                            at.pop();
                        }
                    )+
                }
            }
        )+
    };
}

objects! {
    [A a 0]
    [A a 0, B b 1]
    [A a 0, B b 1, C c 2]
    [A a 0, B b 1, C c 2, D d 3]
    [A a 0, B b 1, C c 2, D d 3, E e 4]
    [A a 0, B b 1, C c 2, D d 3, E e 4, F f 5]
    [A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6]
    [A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7]
    [A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8]
    [A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9]
    [A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9, K k 10]
    [A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7, I i 8, J j 9, K k 10, L l 11]
}

/// A schema that reads text and converts it, for input that carries no
/// types: a form, a query string, an environment variable.
///
/// Conversion is a choice, not a default. A JSON body that sends `"3"` for
/// a number is a bug in the sender, and a schema that hides it makes the bug
/// permanent. A form that sends `3` as text is the format working correctly.
#[derive(Clone, Copy, Debug)]
pub struct FromText<S>(S);

/// Read a whole number out of text.
pub const fn text_int(rules: Int) -> FromText<Int> {
    FromText(rules)
}

/// Read a decimal number out of text.
pub const fn text_real(rules: Real) -> FromText<Real> {
    FromText(rules)
}

/// Read a boolean out of text. A form sends `on`, `true`, `1`, `yes`.
pub const fn text_bool() -> FromText<Bool> {
    FromText(Bool)
}

impl Schema for FromText<Int> {
    type Out = i64;

    fn read(&self, input: Value, at: &mut Path, faults: &mut Faults) -> Option<i64> {
        let v = match input {
            Value::Int(v) => v,
            Value::Text(s) => match s.trim().parse::<i64>() {
                Ok(v) => v,
                Err(_) => {
                    faults.add(Flaw::rule(Code::WrongType, "not a whole number"), at);
                    return None;
                }
            },
            other => {
                wrong_type("a whole number was expected", &other, at, faults);
                return None;
            }
        };
        self.0.read(Value::Int(v), at, faults)
    }

    fn inspect(&self, value: &i64, at: &mut Path, faults: &mut Faults) {
        self.0.inspect(value, at, faults);
    }
}

impl Schema for FromText<Real> {
    type Out = f64;

    fn read(&self, input: Value, at: &mut Path, faults: &mut Faults) -> Option<f64> {
        let v = match input {
            Value::Real(v) => v,
            Value::Int(v) => v as f64,
            Value::Text(s) => match s.trim().parse::<f64>() {
                Ok(v) => v,
                Err(_) => {
                    faults.add(Flaw::rule(Code::WrongType, "not a number"), at);
                    return None;
                }
            },
            other => {
                wrong_type("a number was expected", &other, at, faults);
                return None;
            }
        };
        self.0.read(Value::Real(v), at, faults)
    }

    fn inspect(&self, value: &f64, at: &mut Path, faults: &mut Faults) {
        self.0.inspect(value, at, faults);
    }
}

impl Schema for FromText<Bool> {
    type Out = bool;

    fn read(&self, input: Value, at: &mut Path, faults: &mut Faults) -> Option<bool> {
        match input {
            Value::Bool(b) => Some(b),
            // An unticked checkbox sends nothing at all.
            Value::Null => Some(false),
            Value::Text(s) => match s.trim() {
                "on" | "true" | "yes" | "1" => Some(true),
                "off" | "false" | "no" | "0" => Some(false),
                _ => {
                    faults.add(Flaw::rule(Code::WrongType, "not a boolean"), at);
                    None
                }
            },
            other => {
                wrong_type("a boolean was expected", &other, at, faults);
                None
            }
        }
    }

    fn inspect(&self, _value: &bool, _at: &mut Path, _faults: &mut Faults) {}
}

/// Every fault, grouped by the field it belongs to.
pub fn by_field(faults: &Faults) -> Vec<(String, Vec<String>)> {
    faults.by_path()
}

/// The faults, as a flat list.
pub fn each(faults: &Faults) -> &[Fault] {
    faults.all()
}
