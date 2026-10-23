//! What a schema says when it refuses a value.
//!
//! A [`Flaw`] is the const half: a `Copy` record with no allocation, so a
//! rule can produce one while the compiler evaluates it. A [`Fault`] is the
//! run-time half: a flaw, the path that locates it, and the name a person
//! would use for that place.

use core::fmt;

/// Why a value was refused. One variant per kind of rule, not per rule.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Code {
    /// The value is not of the type the schema reads.
    WrongType,
    /// Below a lower bound: length, count, or magnitude.
    TooSmall,
    /// Above an upper bound.
    TooBig,
    /// Not one of the values the schema lists.
    OutsideSet,
    /// The right type and size, the wrong shape: charset, pattern, format.
    BadShape,
    /// Not a multiple of the step the schema carries.
    NotMultiple,
    /// Infinite or NaN.
    NotFinite,
    /// A required field is absent.
    Missing,
    /// A field the schema does not declare, under `strict`.
    Unexpected,
    /// A rule the caller wrote.
    Custom,
}

impl Code {
    /// The name this code carries in a serialised error, stable across
    /// versions.
    pub const fn name(self) -> &'static str {
        match self {
            Code::WrongType => "wrong_type",
            Code::TooSmall => "too_small",
            Code::TooBig => "too_big",
            Code::OutsideSet => "outside_set",
            Code::BadShape => "bad_shape",
            Code::NotMultiple => "not_multiple",
            Code::NotFinite => "not_finite",
            Code::Missing => "missing",
            Code::Unexpected => "unexpected",
            Code::Custom => "custom",
        }
    }
}

/// One broken rule, with no path and no allocation.
///
/// `says` names the rule in the words of the person who wrote the schema.
/// `limit` and `found` carry the two numbers a bound compares, so a caller
/// can build its own message without parsing `says`. A rule that compares no
/// numbers leaves both at zero.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Flaw {
    /// The kind of rule that refused the value.
    pub code: Code,
    /// What the rule wanted, in one short phrase.
    pub says: &'static str,
    /// The bound the rule carries.
    pub limit: i128,
    /// What the value gave.
    pub found: i128,
}

impl Flaw {
    /// A flaw from a rule that compares no numbers.
    pub const fn rule(code: Code, says: &'static str) -> Flaw {
        Flaw {
            code,
            says,
            limit: 0,
            found: 0,
        }
    }

    /// A flaw from a rule that compares a bound against a value.
    pub const fn bound(code: Code, says: &'static str, limit: i128, found: i128) -> Flaw {
        Flaw {
            code,
            says,
            limit,
            found,
        }
    }

    /// True when the numbers mean something for this code.
    pub const fn counts(&self) -> bool {
        matches!(self.code, Code::TooSmall | Code::TooBig | Code::NotMultiple)
    }
}

impl fmt::Display for Flaw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.counts() {
            write!(f, "{} ({}, found {})", self.says, self.limit, self.found)
        } else {
            f.write_str(self.says)
        }
    }
}

/// One step down into a value.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Step {
    /// A field of an object.
    Key(String),
    /// A position in a list.
    At(usize),
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::Key(k) => f.write_str(k),
            Step::At(i) => write!(f, "{i}"),
        }
    }
}

/// Where a flaw sits inside a value, from the outside in.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Path(Vec<Step>);

impl Path {
    /// The empty path: the value itself.
    pub const fn root() -> Path {
        Path(Vec::new())
    }

    /// Go one step down.
    pub fn push(&mut self, step: Step) {
        self.0.push(step);
    }

    /// Come back one step.
    pub fn pop(&mut self) {
        self.0.pop();
    }

    /// The steps, from the outside in.
    pub fn steps(&self) -> &[Step] {
        &self.0
    }

    /// True for the value itself.
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// The last step, which is the field the fault belongs to.
    pub fn last(&self) -> Option<&Step> {
        self.0.last()
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, step) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(".")?;
            }
            write!(f, "{step}")?;
        }
        Ok(())
    }
}

/// A flaw, the place it was found, and the name a person would use for it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Fault {
    /// The broken rule.
    pub flaw: Flaw,
    /// Where it broke.
    pub path: Path,
    /// What to call that place when a person reads the message. The schema
    /// sets it with `label`; without one, the path is the name.
    pub label: Option<&'static str>,
}

impl Fault {
    /// The name of the place, for a person.
    pub fn name(&self) -> String {
        match self.label {
            Some(label) => label.to_string(),
            None => self.path.to_string(),
        }
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_root() && self.label.is_none() {
            write!(f, "{}", self.flaw)
        } else {
            write!(f, "{}: {}", self.name(), self.flaw)
        }
    }
}

/// Every fault one parse or check found.
///
/// A schema reports all of them. It does not stop at the first, because a
/// form with three bad fields should come back once, not three times.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Faults(Vec<Fault>);

impl Faults {
    /// No faults.
    pub const fn none() -> Faults {
        Faults(Vec::new())
    }

    /// Every flaw, all at the value itself.
    pub fn at_root(flaws: Vec<Flaw>) -> Faults {
        Faults(
            flaws
                .into_iter()
                .map(|flaw| Fault {
                    flaw,
                    path: Path::root(),
                    label: None,
                })
                .collect(),
        )
    }

    /// Record one fault.
    pub fn push(&mut self, fault: Fault) {
        self.0.push(fault);
    }

    /// Take every fault from another set.
    pub fn absorb(&mut self, other: Faults) {
        self.0.extend(other.0);
    }

    /// True when the value passed.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many rules the value broke.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// The faults, in the order they were found.
    pub fn all(&self) -> &[Fault] {
        &self.0
    }

    /// The faults at one place, by the path written with dots.
    pub fn at(&self, path: &str) -> Vec<&Fault> {
        self.0
            .iter()
            .filter(|f| f.path.to_string() == path)
            .collect()
    }

    /// `Ok(())` when the value passed, so a caller can use `?`.
    pub fn ok(self) -> Result<(), Faults> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }

    /// The messages, grouped by path, for a form that shows errors next to
    /// the input that caused them.
    pub fn by_path(&self) -> Vec<(String, Vec<String>)> {
        self.grouped(|fault| fault.path.to_string())
    }

    /// The messages, grouped by the name a person reads.
    pub fn by_name(&self) -> Vec<(String, Vec<String>)> {
        self.grouped(|fault| fault.name())
    }

    fn grouped(&self, key_of: impl Fn(&Fault) -> String) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        for fault in &self.0 {
            let key = key_of(fault);
            match out.iter_mut().find(|(k, _)| *k == key) {
                Some((_, list)) => list.push(fault.flaw.to_string()),
                None => out.push((key, vec![fault.flaw.to_string()])),
            }
        }
        out
    }
}

impl IntoIterator for Faults {
    type Item = Fault;
    type IntoIter = std::vec::IntoIter<Fault>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Faults {
    type Item = &'a Fault;
    type IntoIter = std::slice::Iter<'a, Fault>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl fmt::Display for Faults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.len() {
            0 => f.write_str("no faults"),
            1 => write!(f, "{}", self.0[0]),
            n => {
                write!(f, "{n} faults:")?;
                for fault in &self.0 {
                    write!(f, "\n  {fault}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for Faults {}
