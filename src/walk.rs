//! The state a schema carries while it reads one value.
//!
//! It holds three things a rule cannot work without: where it is inside the
//! value, every fault found so far, and whether the input carries types.

use crate::flaw::{Fault, Faults, Flaw, Path, Step};

/// Whether the input knows its own types.
///
/// A JSON body says `36` for a number and `"36"` for text, and the
/// difference is information. A form says `36` for both, because the format
/// has no types to lose. One schema serves both: it converts under
/// [`Shape::Flat`] and refuses to guess under [`Shape::Typed`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    /// JSON, or any source that kept its types.
    Typed,
    /// A form, a query string, an environment: everything is text.
    Flat,
}

impl Shape {
    /// True when text may be read as a number or a boolean.
    pub const fn converts(self) -> bool {
        matches!(self, Shape::Flat)
    }
}

/// Where the read is, what it found, and what the input is made of.
#[derive(Debug)]
pub struct Walk {
    at: Path,
    faults: Faults,
    shape: Shape,
    label: Option<&'static str>,
    says: Option<&'static str>,
}

impl Walk {
    /// A walk over input of this shape, starting at the value itself.
    pub fn new(shape: Shape) -> Walk {
        Walk {
            at: Path::root(),
            faults: Faults::none(),
            shape,
            label: None,
            says: None,
        }
    }

    /// What the input is made of.
    pub const fn shape(&self) -> Shape {
        self.shape
    }

    /// Where the read currently is.
    pub const fn path(&self) -> &Path {
        &self.at
    }

    /// How many faults the read has found.
    pub fn found(&self) -> usize {
        self.faults.len()
    }

    /// Record a broken rule here.
    ///
    /// A `says` set by [`Walk::with_says`] replaces the rule's own wording,
    /// which is how a field says one thing instead of three.
    pub fn fault(&mut self, flaw: Flaw) {
        let flaw = match self.says {
            Some(says) => Flaw { says, ..flaw },
            None => flaw,
        };
        self.faults.push(Fault {
            flaw,
            path: self.at.clone(),
            label: self.label,
        });
    }

    /// Go one step down, run the body, and come back.
    pub fn down<T>(&mut self, step: Step, body: impl FnOnce(&mut Walk) -> T) -> T {
        self.at.push(step);
        let out = body(self);
        self.at.pop();
        out
    }

    /// Go into a named field.
    pub fn field<T>(&mut self, name: &str, body: impl FnOnce(&mut Walk) -> T) -> T {
        self.down(Step::Key(name.to_string()), body)
    }

    /// Go into one item of a list.
    pub fn item<T>(&mut self, index: usize, body: impl FnOnce(&mut Walk) -> T) -> T {
        self.down(Step::At(index), body)
    }

    /// Run the body as if the input carried no types, so text may become a
    /// number. An API that must keep accepting `"36"` says so here.
    pub fn as_flat<T>(&mut self, body: impl FnOnce(&mut Walk) -> T) -> T {
        let held = std::mem::replace(&mut self.shape, Shape::Flat);
        let out = body(self);
        self.shape = held;
        out
    }

    /// Run the body with this wording in force.
    pub fn with_says<T>(&mut self, says: &'static str, body: impl FnOnce(&mut Walk) -> T) -> T {
        let held = self.says.replace(says);
        let out = body(self);
        self.says = held;
        out
    }

    /// Run the body with this name in force.
    pub fn with_label<T>(&mut self, label: &'static str, body: impl FnOnce(&mut Walk) -> T) -> T {
        let held = self.label.replace(label);
        let out = body(self);
        self.label = held;
        out
    }

    /// Run the body and say whether it added a fault.
    pub fn clean<T>(&mut self, body: impl FnOnce(&mut Walk) -> T) -> (T, bool) {
        let before = self.found();
        let out = body(self);
        (out, self.found() == before)
    }

    /// Every fault the walk found.
    pub fn faults(self) -> Faults {
        self.faults
    }
}
