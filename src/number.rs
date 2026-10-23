//! Rules for numbers, every one of them evaluable by the compiler.
//!
//! ```
//! use zod::Int;
//!
//! const PORT: Int = Int::new().from(1).to(65535);
//!
//! zod::accepts!(PORT, 8080);
//! assert!(PORT.check(0).is_some());
//! ```

use crate::flaw::{Code, Flaw};
use crate::text::refuse;

/// What a whole number must satisfy.
///
/// It compares in `i128`, so every Rust integer up to `i64` and `u64` fits
/// without loss. A `u128` above `i128::MAX` does not, and the schema layer
/// refuses it rather than wrap it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Int {
    min: i128,
    max: i128,
    step: i128,
}

impl Int {
    /// A whole number with no rule but its type.
    pub const fn new() -> Int {
        Int {
            min: i128::MIN,
            max: i128::MAX,
            step: 0,
        }
    }

    /// At least `n`.
    pub const fn from(mut self, n: i128) -> Int {
        self.min = n;
        self
    }

    /// At most `n`.
    pub const fn to(mut self, n: i128) -> Int {
        self.max = n;
        self
    }

    /// Above zero.
    pub const fn positive(self) -> Int {
        self.from(1)
    }

    /// Below zero.
    pub const fn negative(self) -> Int {
        self.to(-1)
    }

    /// Zero or above.
    pub const fn unsigned(self) -> Int {
        self.from(0)
    }

    /// A multiple of `n`. A step of zero means no rule.
    pub const fn step(mut self, n: i128) -> Int {
        self.step = n;
        self
    }

    /// The first rule the number breaks, or `None`.
    pub const fn check(&self, v: i128) -> Option<Flaw> {
        if v < self.min {
            return Some(Flaw::bound(
                Code::TooSmall,
                "below the minimum",
                self.min,
                v,
            ));
        }
        if v > self.max {
            return Some(Flaw::bound(Code::TooBig, "above the maximum", self.max, v));
        }
        if self.step != 0 && v % self.step != 0 {
            return Some(Flaw::bound(
                Code::NotMultiple,
                "not a multiple",
                self.step,
                v,
            ));
        }
        None
    }

    /// True when the number breaks no rule.
    pub const fn accepts(&self, v: i128) -> bool {
        self.check(v).is_none()
    }

    /// Refuse to compile when the literal breaks a rule.
    pub const fn must_accept(&self, v: i128) {
        match self.check(v) {
            None => (),
            Some(flaw) => refuse(flaw.says),
        }
    }

    /// Every rule the number breaks. At most one bound can break, so this
    /// returns at most two: a bound and the step.
    pub fn faults(&self, v: i128) -> Vec<Flaw> {
        let mut out = Vec::new();
        if v < self.min {
            out.push(Flaw::bound(
                Code::TooSmall,
                "below the minimum",
                self.min,
                v,
            ));
        }
        if v > self.max {
            out.push(Flaw::bound(Code::TooBig, "above the maximum", self.max, v));
        }
        if self.step != 0 && v % self.step != 0 {
            out.push(Flaw::bound(
                Code::NotMultiple,
                "not a multiple",
                self.step,
                v,
            ));
        }
        out
    }
}

impl Default for Int {
    fn default() -> Int {
        Int::new()
    }
}

/// What a decimal number must satisfy.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Real {
    min: f64,
    max: f64,
    finite: bool,
}

impl Real {
    /// A decimal number with no rule but its type. It is finite by default,
    /// because `NaN` passing a range check is a bug every time.
    pub const fn new() -> Real {
        Real {
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
            finite: true,
        }
    }

    /// Allow infinity and `NaN`.
    pub const fn wild(mut self) -> Real {
        self.finite = false;
        self
    }

    /// At least `n`.
    pub const fn from(mut self, n: f64) -> Real {
        self.min = n;
        self
    }

    /// At most `n`.
    pub const fn to(mut self, n: f64) -> Real {
        self.max = n;
        self
    }

    /// Above zero.
    pub const fn positive(self) -> Real {
        self.from(f64::MIN_POSITIVE)
    }

    /// The first rule the number breaks, or `None`.
    pub const fn check(&self, v: f64) -> Option<Flaw> {
        if self.finite && !is_finite(v) {
            return Some(Flaw::rule(Code::NotFinite, "not a finite number"));
        }
        if v < self.min {
            return Some(Flaw::bound(
                Code::TooSmall,
                "below the minimum",
                self.min as i128,
                v as i128,
            ));
        }
        if v > self.max {
            return Some(Flaw::bound(
                Code::TooBig,
                "above the maximum",
                self.max as i128,
                v as i128,
            ));
        }
        None
    }

    /// True when the number breaks no rule.
    pub const fn accepts(&self, v: f64) -> bool {
        self.check(v).is_none()
    }

    /// Refuse to compile when the literal breaks a rule.
    pub const fn must_accept(&self, v: f64) {
        match self.check(v) {
            None => (),
            Some(flaw) => refuse(flaw.says),
        }
    }

    /// Every rule the number breaks.
    pub fn faults(&self, v: f64) -> Vec<Flaw> {
        match self.check(v) {
            None => Vec::new(),
            Some(flaw) => vec![flaw],
        }
    }
}

impl Default for Real {
    fn default() -> Real {
        Real::new()
    }
}

/// `NaN` compares false with everything, itself included, which is the only
/// test for it that a const context allows.
#[allow(clippy::eq_op)]
const fn is_finite(v: f64) -> bool {
    v == v && v != f64::INFINITY && v != f64::NEG_INFINITY
}
