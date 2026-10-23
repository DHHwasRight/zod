//! Schemas that read unknown input, and check values the program already
//! holds.
//!
//! Every schema answers the two questions separately:
//!
//! - [`Schema::parse`] takes input whose type nobody has proven and gives
//!   back a Rust value, or every reason it could not.
//! - [`Schema::check`] takes the Rust value itself and runs only the rules.
//!   It cannot fail on the type, because the compiler already settled that.
//!
//! The invariant that holds them together: `check(v).is_empty()` agrees
//! with whether `parse` would accept the input that produces `v`. Neither
//! answer can drift from the other, because both call the same rules.

use std::marker::PhantomData;

use crate::flaw::{Code, Faults, Flaw};
use crate::number::{Int, Real};
use crate::report::Refusal;
use crate::text::Text;
use crate::value::{Input, Value};
use crate::walk::Walk;

/// A rule set that reads one kind of value.
pub trait Schema {
    /// What a successful parse gives back.
    type Out;

    /// Read unknown input at the place the walk sits on.
    ///
    /// It returns `None` only when it has recorded at least one fault.
    fn read(&self, input: Value, walk: &mut Walk) -> Option<Self::Out>;

    /// Run the rules against a value that already has the right type.
    fn inspect(&self, value: &Self::Out, walk: &mut Walk);

    /// Read unknown input, from the top.
    fn parse(&self, input: impl Into<Input>) -> Result<Self::Out, Faults>
    where
        Self: Sized,
    {
        let input = input.into();
        let mut walk = Walk::new(input.shape);
        let out = self.read(input.value, &mut walk);
        let faults = walk.faults();
        match out {
            Some(value) if faults.is_empty() => Ok(value),
            _ => Err(faults),
        }
    }

    /// Read unknown input, and hand every fault to the refusal.
    ///
    /// This is the call a boundary makes. The error type is whatever the
    /// refusal builds: a page, a JSON body, an HTTP answer.
    fn enter<R: Refusal>(&self, input: impl Into<Input>, how: &R) -> Result<Self::Out, R::Out>
    where
        Self: Sized,
    {
        self.parse(input).map_err(|faults| how.refuse(&faults))
    }

    /// Run the rules against a value the program already holds.
    fn check(&self, value: &Self::Out) -> Faults
    where
        Self: Sized,
    {
        let mut walk = Walk::new(crate::walk::Shape::Typed);
        self.inspect(value, &mut walk);
        walk.faults()
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

    /// Replace every message this schema produces with one sentence.
    ///
    /// Three broken rules on one field usually want one message, written
    /// for the person filling the form, not three written for the machine.
    fn says(self, says: &'static str) -> Says<Self>
    where
        Self: Sized,
    {
        Says { inner: self, says }
    }

    /// Give this place a name a person recognises, instead of the path.
    fn label(self, label: &'static str) -> Labelled<Self>
    where
        Self: Sized,
    {
        Labelled { inner: self, label }
    }

    /// Read text as a number or a boolean even when the input carries
    /// types, for an API that must keep accepting what it once accepted.
    fn loose(self) -> Loose<Self>
    where
        Self: Sized,
    {
        Loose { inner: self }
    }
}

/// The value is not there at all.
fn absent(walk: &mut Walk) {
    walk.fault(Flaw::rule(Code::Missing, "required"));
}

fn wrong_type(expected: &'static str, walk: &mut Walk) {
    walk.fault(Flaw::rule(Code::WrongType, expected));
}

/// Report a list of flaws and say whether there were none.
fn note(flaws: Vec<Flaw>, walk: &mut Walk) -> bool {
    let clean = flaws.is_empty();
    for flaw in flaws {
        walk.fault(flaw);
    }
    clean
}

impl Schema for Text {
    type Out = String;

    fn read(&self, input: Value, walk: &mut Walk) -> Option<String> {
        match input {
            Value::Text(s) => note(self.faults(&s), walk).then_some(s),
            Value::Null => {
                absent(walk);
                None
            }
            _ => {
                wrong_type("text was expected", walk);
                None
            }
        }
    }

    fn inspect(&self, value: &String, walk: &mut Walk) {
        note(self.faults(value), walk);
    }
}

/// Pull a whole number out of input, converting text when the source has no
/// types of its own.
fn read_int(input: Value, walk: &mut Walk) -> Option<i128> {
    match input {
        Value::Int(v) => Some(v as i128),
        Value::Text(s) if walk.shape().converts() => match s.trim().parse::<i128>() {
            Ok(v) => Some(v),
            Err(_) => {
                walk.fault(Flaw::rule(Code::WrongType, "not a whole number"));
                None
            }
        },
        Value::Null => {
            absent(walk);
            None
        }
        _ => {
            wrong_type("a whole number was expected", walk);
            None
        }
    }
}

impl Schema for Int {
    type Out = i64;

    fn read(&self, input: Value, walk: &mut Walk) -> Option<i64> {
        let v = read_int(input, walk)?;
        if v < i64::MIN as i128 || v > i64::MAX as i128 {
            walk.fault(Flaw::rule(
                Code::TooBig,
                "outside the range of a whole number",
            ));
            return None;
        }
        note(self.faults(v), walk).then_some(v as i64)
    }

    fn inspect(&self, value: &i64, walk: &mut Walk) {
        note(self.faults(*value as i128), walk);
    }
}

/// An integer of a width other than `i64`.
///
/// `Int` carries the rules; this carries the type the field actually has,
/// and refuses a number that would not survive the conversion.
#[derive(Clone, Copy, Debug)]
pub struct Whole<T> {
    rules: Int,
    kind: PhantomData<T>,
}

/// An integer of this width, with these rules.
pub const fn whole<T>(rules: Int) -> Whole<T> {
    Whole {
        rules,
        kind: PhantomData,
    }
}

impl Int {
    /// These rules, for a field that is not an `i64`.
    ///
    /// ```
    /// use zod::{Int, Schema};
    /// let quantity = Int::new().from(1).to(99).of::<u8>();
    /// assert_eq!(quantity.parse(zod::Value::Int(7)).unwrap(), 7u8);
    /// ```
    pub const fn of<T>(self) -> Whole<T> {
        whole(self)
    }
}

macro_rules! wholes {
    ($($t:ty),+) => {
        $(
            impl Whole<$t> {
                fn judge(&self, raw: i128, walk: &mut Walk) -> Option<$t> {
                    if raw < <$t>::MIN as i128 {
                        walk.fault(Flaw::bound(
                            Code::TooSmall, "below the minimum", <$t>::MIN as i128, raw,
                        ));
                        return None;
                    }
                    if raw > <$t>::MAX as i128 {
                        walk.fault(Flaw::bound(
                            Code::TooBig, "above the maximum", <$t>::MAX as i128, raw,
                        ));
                        return None;
                    }
                    note(self.rules.faults(raw), walk).then_some(raw as $t)
                }
            }

            impl Schema for Whole<$t> {
                type Out = $t;

                fn read(&self, input: Value, walk: &mut Walk) -> Option<$t> {
                    let raw = read_int(input, walk)?;
                    self.judge(raw, walk)
                }

                fn inspect(&self, value: &$t, walk: &mut Walk) {
                    note(self.rules.faults(*value as i128), walk);
                }
            }

            impl Native for $t {
                type By = Whole<$t>;
                fn by() -> Whole<$t> {
                    whole(Int::new())
                }
            }
        )+
    };
}

wholes!(i8, i16, i32, u8, u16, u32, u64, usize, isize);

impl Schema for Real {
    type Out = f64;

    fn read(&self, input: Value, walk: &mut Walk) -> Option<f64> {
        let v = match input {
            Value::Real(v) => v,
            Value::Int(v) => v as f64,
            Value::Text(s) if walk.shape().converts() => match s.trim().parse::<f64>() {
                Ok(v) => v,
                Err(_) => {
                    walk.fault(Flaw::rule(Code::WrongType, "not a number"));
                    return None;
                }
            },
            Value::Null => {
                absent(walk);
                return None;
            }
            _ => {
                wrong_type("a number was expected", walk);
                return None;
            }
        };
        note(self.faults(v), walk).then_some(v)
    }

    fn inspect(&self, value: &f64, walk: &mut Walk) {
        note(self.faults(*value), walk);
    }
}

/// A boolean, which has no rule but its type.
#[derive(Clone, Copy, Default, Debug)]
pub struct Bool;

/// A boolean.
pub const fn boolean() -> Bool {
    Bool
}

impl Schema for Bool {
    type Out = bool;

    fn read(&self, input: Value, walk: &mut Walk) -> Option<bool> {
        match input {
            Value::Bool(b) => Some(b),
            Value::Text(s) if walk.shape().converts() => match s.trim() {
                "on" | "true" | "yes" | "1" => Some(true),
                "off" | "false" | "no" | "0" | "" => Some(false),
                _ => {
                    walk.fault(Flaw::rule(Code::WrongType, "not a boolean"));
                    None
                }
            },
            // An unticked checkbox sends nothing at all, and that is the
            // only way a form can say false.
            Value::Null if walk.shape().converts() => Some(false),
            Value::Null => {
                absent(walk);
                None
            }
            _ => {
                wrong_type("a boolean was expected", walk);
                None
            }
        }
    }

    fn inspect(&self, _value: &bool, _walk: &mut Walk) {}
}

/// A schema that also accepts nothing.
#[derive(Clone, Copy, Debug)]
pub struct Opt<S> {
    inner: S,
}

impl<S: Schema> Schema for Opt<S> {
    type Out = Option<S::Out>;

    fn read(&self, input: Value, walk: &mut Walk) -> Option<Option<S::Out>> {
        match input {
            Value::Null => Some(None),
            other => self.inner.read(other, walk).map(Some),
        }
    }

    fn inspect(&self, value: &Option<S::Out>, walk: &mut Walk) {
        if let Some(inner) = value {
            self.inner.inspect(inner, walk);
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

    fn read(&self, input: Value, walk: &mut Walk) -> Option<S::Out> {
        match input {
            Value::Null => {
                let spare = self.value.clone();
                let (_, clean) = walk.clean(|walk| self.inner.inspect(&spare, walk));
                clean.then_some(spare)
            }
            other => self.inner.read(other, walk),
        }
    }

    fn inspect(&self, value: &S::Out, walk: &mut Walk) {
        self.inner.inspect(value, walk);
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

    fn read(&self, input: Value, walk: &mut Walk) -> Option<S::Out> {
        let out = self.inner.read(input, walk)?;
        if (self.test)(&out) {
            Some(out)
        } else {
            walk.fault(Flaw::rule(Code::Custom, self.says));
            None
        }
    }

    fn inspect(&self, value: &S::Out, walk: &mut Walk) {
        self.inner.inspect(value, walk);
        if !(self.test)(value) {
            walk.fault(Flaw::rule(Code::Custom, self.says));
        }
    }
}

/// A schema whose faults all carry one sentence.
#[derive(Clone, Copy, Debug)]
pub struct Says<S> {
    inner: S,
    says: &'static str,
}

impl<S: Schema> Schema for Says<S> {
    type Out = S::Out;

    fn read(&self, input: Value, walk: &mut Walk) -> Option<S::Out> {
        walk.with_says(self.says, |walk| self.inner.read(input, walk))
    }

    fn inspect(&self, value: &S::Out, walk: &mut Walk) {
        walk.with_says(self.says, |walk| self.inner.inspect(value, walk));
    }
}

/// A schema whose place has a name a person recognises.
#[derive(Clone, Copy, Debug)]
pub struct Labelled<S> {
    inner: S,
    label: &'static str,
}

impl<S: Schema> Schema for Labelled<S> {
    type Out = S::Out;

    fn read(&self, input: Value, walk: &mut Walk) -> Option<S::Out> {
        walk.with_label(self.label, |walk| self.inner.read(input, walk))
    }

    fn inspect(&self, value: &S::Out, walk: &mut Walk) {
        walk.with_label(self.label, |walk| self.inner.inspect(value, walk));
    }
}

/// A schema that converts text even when the input kept its types.
#[derive(Clone, Copy, Debug)]
pub struct Loose<S> {
    inner: S,
}

impl<S: Schema> Schema for Loose<S> {
    type Out = S::Out;

    fn read(&self, input: Value, walk: &mut Walk) -> Option<S::Out> {
        walk.as_flat(|walk| self.inner.read(input, walk))
    }

    fn inspect(&self, value: &S::Out, walk: &mut Walk) {
        self.inner.inspect(value, walk);
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

    fn count(&self, n: usize, walk: &mut Walk) {
        if n < self.min {
            walk.fault(Flaw::bound(
                Code::TooSmall,
                "too few items",
                self.min as i128,
                n as i128,
            ));
        }
        if n > self.max {
            walk.fault(Flaw::bound(
                Code::TooBig,
                "too many items",
                self.max as i128,
                n as i128,
            ));
        }
    }
}

impl<S: Schema> Schema for List<S> {
    type Out = Vec<S::Out>;

    fn read(&self, input: Value, walk: &mut Walk) -> Option<Vec<S::Out>> {
        let items = match input {
            Value::List(items) => items,
            // A form names the field once per value, so one value is a list
            // of one, and no value at all is an empty list.
            Value::Null if walk.shape().converts() => Vec::new(),
            other if walk.shape().converts() => vec![other],
            Value::Null => {
                absent(walk);
                return None;
            }
            _ => {
                wrong_type("a list was expected", walk);
                return None;
            }
        };

        let (out, clean) = walk.clean(|walk| {
            self.count(items.len(), walk);
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.into_iter().enumerate() {
                if let Some(value) = walk.item(i, |walk| self.item.read(item, walk)) {
                    out.push(value);
                }
            }
            out
        });

        clean.then_some(out)
    }

    #[allow(clippy::ptr_arg)]
    fn inspect(&self, value: &Vec<S::Out>, walk: &mut Walk) {
        self.count(value.len(), walk);
        for (i, item) in value.iter().enumerate() {
            walk.item(i, |walk| self.item.inspect(item, walk));
        }
    }
}

/// Named fields, read into a tuple.
///
/// The output type comes out of the schema with no derive, which is the
/// part TypeScript needs `z.infer` for. For a real struct, derive
/// [`Entry`] instead.
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

/// Take the map apart, or say why it is not one.
///
/// `#[derive(Entry)]` calls it, which is why it is public.
pub fn open_map(input: Value, walk: &mut Walk) -> Option<Value> {
    match input {
        map @ Value::Map(_) => Some(map),
        Value::Null => {
            absent(walk);
            None
        }
        _ => {
            wrong_type("an object was expected", walk);
            None
        }
    }
}

/// Report every field the schema does not declare.
pub fn refuse_spare(left: &Value, walk: &mut Walk) {
    for key in left.spare_keys() {
        walk.field(&key, |walk| {
            walk.fault(Flaw::rule(Code::Unexpected, "unknown field"))
        });
    }
}

macro_rules! objects {
    ($( [$($t:ident $v:ident $i:tt),+] )+) => {
        $(
            impl<$($t: Schema),+> Schema for Obj<($((&'static str, $t),)+)> {
                type Out = ($($t::Out,)+);

                fn read(&self, input: Value, walk: &mut Walk) -> Option<Self::Out> {
                    let mut input = open_map(input, walk)?;
                    let before = walk.found();
                    $(
                        let $v = {
                            let (name, schema) = &self.fields.$i;
                            let field = input.take(name);
                            walk.field(name, |walk| schema.read(field, walk))
                        };
                    )+
                    if self.strict {
                        refuse_spare(&input, walk);
                    }
                    if walk.found() != before {
                        return None;
                    }
                    Some(($($v?,)+))
                }

                fn inspect(&self, value: &Self::Out, walk: &mut Walk) {
                    $({
                        let (name, schema) = &self.fields.$i;
                        walk.field(name, |walk| schema.inspect(&value.$i, walk));
                    })+
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

/// The schema a type uses when the field says nothing else.
///
/// It is what lets `#[derive(Entry)]` read a plain `String` field with no
/// attribute at all, and what makes `Option<T>` and `Vec<T>` work for every
/// `T` that has one.
pub trait Native: Sized {
    /// The schema that reads this type.
    type By: Schema<Out = Self>;

    /// That schema, with no rule but the type.
    fn by() -> Self::By;
}

impl Native for String {
    type By = Text;
    fn by() -> Text {
        Text::new()
    }
}

impl Native for i64 {
    type By = Int;
    fn by() -> Int {
        Int::new()
    }
}

impl Native for f64 {
    type By = Real;
    fn by() -> Real {
        Real::new()
    }
}

impl Native for bool {
    type By = Bool;
    fn by() -> Bool {
        Bool
    }
}

impl<T: Native> Native for Option<T> {
    type By = Opt<T::By>;
    fn by() -> Opt<T::By> {
        Opt { inner: T::by() }
    }
}

impl<T: Native> Native for Vec<T> {
    type By = List<T::By>;
    fn by() -> List<T::By> {
        list(T::by())
    }
}

/// A struct that guards an entrance.
///
/// `#[derive(Entry)]` writes it. The type is the schema: the fields carry
/// the rules, the struct carries the refusal, and nothing at the call site
/// has to remember either.
pub trait Entry: Sized {
    /// What this entrance gives back when it refuses.
    type Deny: Refusal + Default;

    /// Read the struct out of unknown input.
    fn take(input: Value, walk: &mut Walk) -> Option<Self>;

    /// Run the rules against a struct the program already holds.
    fn look(&self, walk: &mut Walk);

    /// Read unknown input, keeping every fault.
    fn parse(input: impl Into<Input>) -> Result<Self, Faults> {
        let input = input.into();
        let mut walk = Walk::new(input.shape);
        let out = Self::take(input.value, &mut walk);
        let faults = walk.faults();
        match out {
            Some(value) if faults.is_empty() => Ok(value),
            _ => Err(faults),
        }
    }

    /// Read unknown input, and hand every fault to the refusal this
    /// entrance carries. This is the call a boundary makes.
    fn enter(input: impl Into<Input>) -> Result<Self, <Self::Deny as Refusal>::Out> {
        Self::parse(input).map_err(|faults| Self::Deny::default().refuse(&faults))
    }

    /// Run the rules against a struct the program already holds.
    fn check(&self) -> Faults {
        let mut walk = Walk::new(crate::walk::Shape::Typed);
        self.look(&mut walk);
        walk.faults()
    }

    /// True when the struct breaks no rule.
    fn sound(&self) -> bool {
        self.check().is_empty()
    }
}

/// A struct read as a field of another schema.
#[derive(Debug)]
pub struct Nested<T>(PhantomData<T>);

impl<T> Clone for Nested<T> {
    fn clone(&self) -> Nested<T> {
        *self
    }
}

impl<T> Copy for Nested<T> {}

/// Read a struct that derives [`Entry`] as a field of another schema.
pub const fn nested<T: Entry>() -> Nested<T> {
    Nested(PhantomData)
}

impl<T: Entry> Schema for Nested<T> {
    type Out = T;

    fn read(&self, input: Value, walk: &mut Walk) -> Option<T> {
        T::take(input, walk)
    }

    fn inspect(&self, value: &T, walk: &mut Walk) {
        value.look(walk);
    }
}
