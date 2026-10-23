//! What happens when the entrance refuses.
//!
//! A schema that only returns faults pushes the same work onto every call
//! site: turn them into a message, choose a status, build a page. A
//! [`Refusal`] is that work, written once and carried by the schema, so no
//! boundary in the program can forget it or do it differently.

use crate::flaw::{Code, Fault, Faults};

/// How a fault reads to a person.
///
/// The default puts the name of the place in front of the rule. Write your
/// own for another language, or for wording your users expect.
pub trait Wording {
    /// One fault, in words.
    fn say(&self, fault: &Fault) -> String {
        format!("{}: {}", self.name(fault), self.rule(fault))
    }

    /// The name of the place the fault sits in.
    fn name(&self, fault: &Fault) -> String {
        fault.name()
    }

    /// The broken rule, without the place.
    fn rule(&self, fault: &Fault) -> String {
        fault.flaw.to_string()
    }
}

/// The wording the crate ships with: the rule as the schema worded it.
#[derive(Clone, Copy, Default, Debug)]
pub struct Plain;

impl Wording for Plain {}

/// Every fault in words, paired with the place it belongs to.
pub fn say_all<W: Wording>(faults: &Faults, wording: &W) -> Vec<(String, String)> {
    faults
        .all()
        .iter()
        .map(|f| (wording.name(f), wording.rule(f)))
        .collect()
}

/// What a refused entrance gives back.
///
/// Implement it for an error page, a JSON body, an HTTP response, a log
/// line: anything the boundary owes its caller. Any `Fn(&Faults) -> T` is
/// one already, so a closure needs no type of its own.
pub trait Refusal {
    /// What the boundary returns when it refuses.
    type Out;

    /// Turn every fault into that.
    fn refuse(&self, faults: &Faults) -> Self::Out;
}

impl<T, F: Fn(&Faults) -> T> Refusal for F {
    type Out = T;

    fn refuse(&self, faults: &Faults) -> T {
        self(faults)
    }
}

/// Hand back the faults untouched, for a caller that will word them itself.
#[derive(Clone, Copy, Default, Debug)]
pub struct Raw;

impl Refusal for Raw {
    type Out = Faults;

    fn refuse(&self, faults: &Faults) -> Faults {
        faults.clone()
    }
}

/// One line per fault, for a log or a terminal.
#[derive(Clone, Copy, Default, Debug)]
pub struct Summary;

impl Refusal for Summary {
    type Out = String;

    fn refuse(&self, faults: &Faults) -> String {
        faults.to_string()
    }
}

/// The messages grouped by field, for a form that redraws itself with the
/// errors next to the inputs that caused them.
#[derive(Clone, Copy, Default, Debug)]
pub struct Fields;

impl Refusal for Fields {
    type Out = Vec<(String, Vec<String>)>;

    fn refuse(&self, faults: &Faults) -> Vec<(String, Vec<String>)> {
        faults.by_path()
    }
}

/// An answer a web boundary can send as it stands.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Refused {
    /// The HTTP status.
    pub status: u16,
    /// The media type of the body.
    pub content_type: &'static str,
    /// The body.
    pub body: String,
}

/// A refusal as `application/problem+json`, by RFC 9457, with one entry per
/// fault under `errors`.
///
/// 422 is the status, because the request parsed and the content is what
/// the server will not accept. A request that did not parse never reaches a
/// schema.
#[derive(Clone, Copy, Default, Debug)]
pub struct Problem;

impl Refusal for Problem {
    type Out = Refused;

    fn refuse(&self, faults: &Faults) -> Refused {
        let mut body = String::from("{\"title\":\"The request did not pass validation\"");
        body.push_str(",\"status\":422,\"errors\":[");
        for (i, fault) in faults.all().iter().enumerate() {
            if i > 0 {
                body.push(',');
            }
            body.push_str("{\"path\":");
            quote(&fault.path.to_string(), &mut body);
            body.push_str(",\"name\":");
            quote(&fault.name(), &mut body);
            body.push_str(",\"code\":");
            quote(fault.flaw.code.name(), &mut body);
            body.push_str(",\"detail\":");
            quote(&fault.flaw.to_string(), &mut body);
            if fault.flaw.counts() {
                body.push_str(&format!(
                    ",\"limit\":{},\"found\":{}",
                    fault.flaw.limit, fault.flaw.found
                ));
            }
            body.push('}');
        }
        body.push_str("]}");
        Refused {
            status: 422,
            content_type: "application/problem+json",
            body,
        }
    }
}

/// JSON string escaping, enough for a message and a path. No dependency,
/// because a refusal must work when the `json` feature is off.
fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// True when every fault is a missing field, which a boundary may want to
/// answer with a form rather than an error.
pub fn all_missing(faults: &Faults) -> bool {
    !faults.is_empty() && faults.all().iter().all(|f| f.flaw.code == Code::Missing)
}
