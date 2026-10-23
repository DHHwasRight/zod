//! What a schema says about itself.
//!
//! A schema checks a value, and that is what it is for. A description is the
//! other half: an OpenAPI document, a command-line flag, a form widget and a
//! client library are all readings of the same rules, and a program that
//! cannot read them has to be told twice.
//!
//! ```
//! use zod::{Entry, Int, Node, Text};
//!
//! #[derive(Entry)]
//! struct Signup {
//!     #[check(Text::new().min(3).max(16).slug())]
//!     #[label("Handle")]
//!     handle: String,
//!     #[check(Int::new().from(13).to(130))]
//!     age: i64,
//!     note: Option<String>,
//! }
//!
//! let node = Signup::describe();
//! assert_eq!(node.name(), Some("Signup"));
//!
//! let object = node.object().unwrap();
//! assert_eq!(object.fields.len(), 3);
//! assert_eq!(object.fields[0].label, Some("Handle"));
//! assert!(object.fields[0].required);
//! assert!(!object.fields[2].required, "an Option is not required");
//! ```
//!
//! The tree owns its children and nothing in it is a reference, so a reader
//! walks it without a registry and without checking for cycles. A name comes
//! from the one place that has one: `#[derive(Entry)]` knows what the struct
//! is called, so a document can say `Signup` rather than inventing a name
//! out of a Rust type path.

use crate::number::{IntFacts, RealFacts};
use crate::text::TextFacts;

/// What a schema allows, as data a program can read.
#[derive(Clone, PartialEq, Debug)]
pub enum Node {
    /// Text, and the rules it carries.
    Text(TextFacts),
    /// A whole number. The bounds are already narrowed to the Rust type
    /// that holds it, so a `u8` field says 0 to 255 whatever the rule said.
    Int(IntFacts),
    /// A decimal number.
    Real(RealFacts),
    /// A boolean, which has no rule but its type.
    Bool,
    /// A list, and how many items it may hold.
    List {
        /// What each item must be.
        item: Box<Node>,
        /// Fewest items.
        min: usize,
        /// Most items. [`usize::MAX`] for no rule.
        max: usize,
    },
    /// Named fields.
    Object(Object),
    /// The value, or nothing.
    Optional(Box<Node>),
    /// The value, with something to use when nothing arrives.
    Defaulted(Box<Node>),
    /// A rule the caller wrote, which only the program that wrote it can
    /// express. A reader keeps the sentence and the inner node.
    Rule {
        /// What the rule allows, as far as this crate can say.
        inner: Box<Node>,
        /// What the rule says when it refuses.
        says: &'static str,
    },
    /// A message that replaces every message the inner node produces.
    Said {
        /// What is described.
        inner: Box<Node>,
        /// The one sentence.
        says: &'static str,
    },
    /// A name a person reads, instead of the path.
    Labelled {
        /// What is described.
        inner: Box<Node>,
        /// The name.
        label: &'static str,
    },
    /// A struct, with the name it has in the source.
    Named {
        /// The struct's name, as a document should print it.
        name: &'static str,
        /// Its shape.
        inner: Box<Node>,
    },
}

/// Named fields, and whether an unknown one is refused.
#[derive(Clone, PartialEq, Debug)]
pub struct Object {
    /// The fields, in the order the struct declares them.
    pub fields: Vec<Field>,
    /// Whether a field the schema does not declare is refused.
    pub strict: bool,
}

/// One field of an object.
#[derive(Clone, PartialEq, Debug)]
pub struct Field {
    /// The name on the wire.
    pub name: &'static str,
    /// The name a person reads, when the schema gave one.
    pub label: Option<&'static str>,
    /// The one sentence that replaces every message, when the schema gave
    /// one. A document can use it as the field's description.
    pub says: Option<&'static str>,
    /// Whether the field must be there.
    ///
    /// Derived rather than declared: a field is required unless its schema
    /// accepts nothing. Two facts that can disagree are two facts that will.
    pub required: bool,
    /// What the field must be, with the label and the message peeled off.
    pub node: Node,
}

impl Field {
    /// Build a field, peeling a label and a message off the node.
    ///
    /// They are properties of the field for a reader, and wrappers around
    /// the schema for a checker. This is the one place the two meet.
    pub fn of(name: &'static str, node: Node) -> Field {
        let mut label = None;
        let mut says = None;
        let mut node = node;
        loop {
            match node {
                Node::Labelled {
                    inner,
                    label: found,
                } => {
                    label = label.or(Some(found));
                    node = *inner;
                }
                Node::Said { inner, says: found } => {
                    says = says.or(Some(found));
                    node = *inner;
                }
                other => {
                    node = other;
                    break;
                }
            }
        }
        Field {
            name,
            label,
            says,
            required: !node.optional(),
            node,
        }
    }
}

impl Node {
    /// Wrap a node in a name.
    pub fn named(name: &'static str, inner: Node) -> Node {
        Node::Named {
            name,
            inner: Box::new(inner),
        }
    }

    /// The name this node carries, if it has one.
    pub fn name(&self) -> Option<&'static str> {
        match self {
            Node::Named { name, .. } => Some(name),
            Node::Optional(inner) | Node::Defaulted(inner) => inner.name(),
            Node::Rule { inner, .. } | Node::Said { inner, .. } => inner.name(),
            Node::Labelled { inner, .. } => inner.name(),
            _ => None,
        }
    }

    /// The object underneath, past any name or wrapper.
    pub fn object(&self) -> Option<&Object> {
        match self {
            Node::Object(object) => Some(object),
            Node::Named { inner, .. }
            | Node::Optional(inner)
            | Node::Defaulted(inner)
            | Node::Rule { inner, .. }
            | Node::Said { inner, .. }
            | Node::Labelled { inner, .. } => inner.object(),
            _ => None,
        }
    }

    /// Whether the schema accepts nothing at all.
    ///
    /// A default counts: a field with one may be left out, which is the
    /// question a document's `required` list asks.
    pub fn optional(&self) -> bool {
        match self {
            Node::Optional(_) | Node::Defaulted(_) => true,
            Node::Named { inner, .. }
            | Node::Rule { inner, .. }
            | Node::Said { inner, .. }
            | Node::Labelled { inner, .. } => inner.optional(),
            _ => false,
        }
    }

    /// The node with every wrapper that does not change the shape removed.
    ///
    /// A reader that wants to know "is this text, and how long" calls this
    /// first. One that wants the name or the message does not.
    pub fn bare(&self) -> &Node {
        match self {
            Node::Named { inner, .. }
            | Node::Optional(inner)
            | Node::Defaulted(inner)
            | Node::Rule { inner, .. }
            | Node::Said { inner, .. }
            | Node::Labelled { inner, .. } => inner.bare(),
            other => other,
        }
    }

    /// Every message the schema carries for this node, outermost first.
    pub fn says(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        let mut node = self;
        loop {
            match node {
                Node::Said { inner, says } => {
                    out.push(*says);
                    node = inner;
                }
                Node::Rule { inner, says } => {
                    out.push(*says);
                    node = inner;
                }
                Node::Named { inner, .. }
                | Node::Optional(inner)
                | Node::Defaulted(inner)
                | Node::Labelled { inner, .. } => node = inner,
                _ => return out,
            }
        }
    }

    /// Every named struct in the tree, outermost first, with no repeats.
    ///
    /// What a document's `components/schemas` section is built from: each
    /// one is written once and referred to by name everywhere else.
    pub fn named_parts(&self) -> Vec<(&'static str, &Node)> {
        let mut out = Vec::new();
        self.gather(&mut out);
        out
    }

    fn gather<'a>(&'a self, out: &mut Vec<(&'static str, &'a Node)>) {
        match self {
            Node::Named { name, inner } => {
                if !out.iter().any(|(seen, _)| seen == name) {
                    out.push((name, inner));
                }
                inner.gather(out);
            }
            Node::Object(object) => {
                for field in &object.fields {
                    field.node.gather(out);
                }
            }
            Node::List { item, .. } => item.gather(out),
            Node::Optional(inner)
            | Node::Defaulted(inner)
            | Node::Rule { inner, .. }
            | Node::Said { inner, .. }
            | Node::Labelled { inner, .. } => inner.gather(out),
            _ => {}
        }
    }
}
