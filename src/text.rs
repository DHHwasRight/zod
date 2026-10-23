//! Rules for text, every one of them evaluable by the compiler.
//!
//! ```
//! use zod::Text;
//!
//! const HANDLE: Text = Text::new().min(3).max(16).slug();
//!
//! zod::accepts!(HANDLE, "ada-lovelace");
//! assert!(HANDLE.check("no").is_some());
//! ```
//!
//! Every rule here runs in a `const` context, so a literal that breaks one
//! is a compile error, not a test failure. That is the whole reason this
//! layer holds no regular expression: a regex engine cannot run inside the
//! compiler, and a rule that only works at run time belongs one layer up.

use crate::flaw::{Code, Flaw};

/// The shape the characters must take.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    /// Any character.
    Free,
    /// ASCII only.
    Ascii,
    /// ASCII letters and digits.
    Alnum,
    /// Digits only.
    Digits,
    /// Hexadecimal digits, either case.
    Hex,
    /// No uppercase ASCII letter.
    Lower,
    /// No lowercase ASCII letter.
    Upper,
    /// Lowercase letters, digits and inner hyphens.
    Slug,
    /// One `@`, a non-empty local part, a domain with an inner dot.
    Email,
    /// The 8-4-4-4-12 hexadecimal form.
    Uuid,
    /// `YYYY-MM-DD`, with a month and a day in range.
    DateIso,
}

impl Shape {
    const fn says(self) -> &'static str {
        match self {
            Shape::Free => "any text",
            Shape::Ascii => "ASCII only",
            Shape::Alnum => "letters and digits only",
            Shape::Digits => "digits only",
            Shape::Hex => "hexadecimal only",
            Shape::Lower => "lowercase only",
            Shape::Upper => "uppercase only",
            Shape::Slug => "a slug: lowercase, digits, inner hyphens",
            Shape::Email => "an email address",
            Shape::Uuid => "a UUID",
            Shape::DateIso => "a date, YYYY-MM-DD",
        }
    }
}

/// What a piece of text must satisfy.
///
/// Build it with `const` methods, store it in a `const`, and the same value
/// serves the compiler and the run time.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Text {
    min: usize,
    max: usize,
    shape: Shape,
    starts: Option<&'static str>,
    ends: Option<&'static str>,
    holds: Option<&'static str>,
}

impl Text {
    /// Text with no rule but its type.
    pub const fn new() -> Text {
        Text {
            min: 0,
            max: usize::MAX,
            shape: Shape::Free,
            starts: None,
            ends: None,
            holds: None,
        }
    }

    /// At least `n` characters, counted as Unicode scalars, not bytes.
    pub const fn min(mut self, n: usize) -> Text {
        self.min = n;
        self
    }

    /// At most `n` characters.
    pub const fn max(mut self, n: usize) -> Text {
        self.max = n;
        self
    }

    /// Exactly `n` characters.
    pub const fn len(mut self, n: usize) -> Text {
        self.min = n;
        self.max = n;
        self
    }

    /// At least one character.
    pub const fn filled(self) -> Text {
        self.min(1)
    }

    /// Only ASCII.
    pub const fn ascii(mut self) -> Text {
        self.shape = Shape::Ascii;
        self
    }

    /// Only ASCII letters and digits.
    pub const fn alnum(mut self) -> Text {
        self.shape = Shape::Alnum;
        self
    }

    /// Only digits.
    pub const fn digits(mut self) -> Text {
        self.shape = Shape::Digits;
        self
    }

    /// Only hexadecimal digits.
    pub const fn hex(mut self) -> Text {
        self.shape = Shape::Hex;
        self
    }

    /// No uppercase ASCII letter.
    pub const fn lower(mut self) -> Text {
        self.shape = Shape::Lower;
        self
    }

    /// No lowercase ASCII letter.
    pub const fn upper(mut self) -> Text {
        self.shape = Shape::Upper;
        self
    }

    /// A slug: lowercase letters, digits, and hyphens between them.
    pub const fn slug(mut self) -> Text {
        self.shape = Shape::Slug;
        self
    }

    /// An email address, by the rule that catches real mistakes: one `@`, a
    /// local part, a domain with an inner dot, and no space anywhere.
    ///
    /// It does not implement RFC 5322. No library does, and the ones that
    /// claim to refuse addresses that deliver mail.
    pub const fn email(mut self) -> Text {
        self.shape = Shape::Email;
        self
    }

    /// The 8-4-4-4-12 hexadecimal form, hyphens included.
    pub const fn uuid(mut self) -> Text {
        self.shape = Shape::Uuid;
        self
    }

    /// A calendar date, `YYYY-MM-DD`.
    pub const fn date(mut self) -> Text {
        self.shape = Shape::DateIso;
        self
    }

    /// Must start with this text.
    pub const fn starts(mut self, with: &'static str) -> Text {
        self.starts = Some(with);
        self
    }

    /// Must end with this text.
    pub const fn ends(mut self, with: &'static str) -> Text {
        self.ends = Some(with);
        self
    }

    /// Must hold this text somewhere.
    pub const fn holds(mut self, part: &'static str) -> Text {
        self.holds = Some(part);
        self
    }

    /// The first rule the text breaks, or `None`.
    ///
    /// It stops at the first, because a const context can report one. The
    /// schema layer calls [`Text::faults`] instead, which reports them all.
    pub const fn check(&self, s: &str) -> Option<Flaw> {
        let b = s.as_bytes();
        let n = count_chars(b);

        if n < self.min {
            return Some(Flaw::bound(
                Code::TooSmall,
                "too short",
                self.min as i128,
                n as i128,
            ));
        }
        if n > self.max {
            return Some(Flaw::bound(
                Code::TooBig,
                "too long",
                self.max as i128,
                n as i128,
            ));
        }
        if let Some(prefix) = self.starts {
            if !starts_with(b, prefix.as_bytes()) {
                return Some(Flaw::rule(Code::BadShape, "wrong prefix"));
            }
        }
        if let Some(suffix) = self.ends {
            if !ends_with(b, suffix.as_bytes()) {
                return Some(Flaw::rule(Code::BadShape, "wrong suffix"));
            }
        }
        if let Some(part) = self.holds {
            if !holds(b, part.as_bytes()) {
                return Some(Flaw::rule(Code::BadShape, "missing part"));
            }
        }
        if !shaped(b, self.shape) {
            return Some(Flaw::rule(Code::BadShape, self.shape.says()));
        }
        None
    }

    /// True when the text breaks no rule.
    pub const fn accepts(&self, s: &str) -> bool {
        self.check(s).is_none()
    }

    /// Refuse to compile when the literal breaks a rule.
    ///
    /// Call it through [`accepts!`](crate::accepts), which gives the macro a
    /// name the error message can carry.
    #[track_caller]
    pub const fn must_accept(&self, s: &str) {
        match self.check(s) {
            None => (),
            Some(flaw) => refuse(flaw.says),
        }
    }

    /// Every rule the text breaks, not only the first.
    ///
    /// The const checker reports one flaw because a compile error needs one
    /// message. A form needs them all.
    pub fn faults(&self, s: &str) -> Vec<Flaw> {
        let b = s.as_bytes();
        let n = count_chars(b);
        let mut out = Vec::new();

        if n < self.min {
            out.push(Flaw::bound(
                Code::TooSmall,
                "too short",
                self.min as i128,
                n as i128,
            ));
        }
        if n > self.max {
            out.push(Flaw::bound(
                Code::TooBig,
                "too long",
                self.max as i128,
                n as i128,
            ));
        }
        if let Some(prefix) = self.starts {
            if !starts_with(b, prefix.as_bytes()) {
                out.push(Flaw::rule(Code::BadShape, "wrong prefix"));
            }
        }
        if let Some(suffix) = self.ends {
            if !ends_with(b, suffix.as_bytes()) {
                out.push(Flaw::rule(Code::BadShape, "wrong suffix"));
            }
        }
        if let Some(part) = self.holds {
            if !holds(b, part.as_bytes()) {
                out.push(Flaw::rule(Code::BadShape, "missing part"));
            }
        }
        if !shaped(b, self.shape) {
            out.push(Flaw::rule(Code::BadShape, self.shape.says()));
        }
        out
    }
}

impl Default for Text {
    fn default() -> Text {
        Text::new()
    }
}

/// Every rule a [`Text`] carries, as plain data.
///
/// The rules are private on `Text` so a builder method and a reader cannot
/// share a name. A reader still has to exist, because a document generator
/// and a command-line builder both have to see what a field allows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TextFacts {
    /// Fewest characters. Zero for no rule.
    pub min: usize,
    /// Most characters. [`usize::MAX`] for no rule.
    pub max: usize,
    /// The shape the characters must take.
    pub shape: Shape,
    /// What the text must start with.
    pub starts: Option<&'static str>,
    /// What the text must end with.
    pub ends: Option<&'static str>,
    /// What the text must hold somewhere.
    pub holds: Option<&'static str>,
}

impl Text {
    /// Every rule this carries, for a reader rather than a checker.
    pub const fn facts(&self) -> TextFacts {
        TextFacts {
            min: self.min,
            max: self.max,
            shape: self.shape,
            starts: self.starts,
            ends: self.ends,
            holds: self.holds,
        }
    }
}

/// Stop the compile. Separate from `must_accept` so the panic sits in one
/// place and the message stays a plain string.
pub(crate) const fn refuse(says: &'static str) -> ! {
    panic!("{}", says)
}

/// Unicode scalars, counted by their lead bytes.
const fn count_chars(b: &[u8]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < b.len() {
        if (b[i] & 0xC0) != 0x80 {
            n += 1;
        }
        i += 1;
    }
    n
}

const fn starts_with(b: &[u8], p: &[u8]) -> bool {
    if p.len() > b.len() {
        return false;
    }
    let mut i = 0;
    while i < p.len() {
        if b[i] != p[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn ends_with(b: &[u8], p: &[u8]) -> bool {
    if p.len() > b.len() {
        return false;
    }
    let off = b.len() - p.len();
    let mut i = 0;
    while i < p.len() {
        if b[off + i] != p[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn holds(b: &[u8], p: &[u8]) -> bool {
    if p.is_empty() {
        return true;
    }
    if p.len() > b.len() {
        return false;
    }
    let last = b.len() - p.len();
    let mut at = 0;
    while at <= last {
        let mut i = 0;
        let mut same = true;
        while i < p.len() {
            if b[at + i] != p[i] {
                same = false;
                break;
            }
            i += 1;
        }
        if same {
            return true;
        }
        at += 1;
    }
    false
}

const fn is_digit(c: u8) -> bool {
    c >= b'0' && c <= b'9'
}

const fn is_hex(c: u8) -> bool {
    is_digit(c) || (c >= b'a' && c <= b'f') || (c >= b'A' && c <= b'F')
}

const fn is_alpha(c: u8) -> bool {
    (c >= b'a' && c <= b'z') || (c >= b'A' && c <= b'Z')
}

const fn shaped(b: &[u8], shape: Shape) -> bool {
    match shape {
        Shape::Free => true,
        Shape::Ascii => all_ascii(b),
        Shape::Alnum => every(b, 0),
        Shape::Digits => every(b, 1),
        Shape::Hex => every(b, 2),
        Shape::Lower => no_upper(b),
        Shape::Upper => no_lower(b),
        Shape::Slug => is_slug(b),
        Shape::Email => is_email(b),
        Shape::Uuid => is_uuid(b),
        Shape::DateIso => is_date(b),
    }
}

const fn all_ascii(b: &[u8]) -> bool {
    let mut i = 0;
    while i < b.len() {
        if b[i] > 127 {
            return false;
        }
        i += 1;
    }
    true
}

/// `kind`: 0 alphanumeric, 1 digits, 2 hexadecimal. A small integer because
/// a const fn cannot take a function pointer and call it.
const fn every(b: &[u8], kind: u8) -> bool {
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let ok = match kind {
            0 => is_alpha(c) || is_digit(c),
            1 => is_digit(c),
            _ => is_hex(c),
        };
        if !ok {
            return false;
        }
        i += 1;
    }
    true
}

const fn no_upper(b: &[u8]) -> bool {
    let mut i = 0;
    while i < b.len() {
        if b[i] >= b'A' && b[i] <= b'Z' {
            return false;
        }
        i += 1;
    }
    true
}

const fn no_lower(b: &[u8]) -> bool {
    let mut i = 0;
    while i < b.len() {
        if b[i] >= b'a' && b[i] <= b'z' {
            return false;
        }
        i += 1;
    }
    true
}

const fn is_slug(b: &[u8]) -> bool {
    if b.is_empty() {
        return false;
    }
    if b[0] == b'-' || b[b.len() - 1] == b'-' {
        return false;
    }
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let ok = is_digit(c) || (c >= b'a' && c <= b'z') || c == b'-';
        if !ok {
            return false;
        }
        if c == b'-' && i + 1 < b.len() && b[i + 1] == b'-' {
            return false;
        }
        i += 1;
    }
    true
}

const fn is_email(b: &[u8]) -> bool {
    let mut at = usize::MAX;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b' ' || c == b'\t' {
            return false;
        }
        if c == b'@' {
            if at != usize::MAX {
                return false;
            }
            at = i;
        }
        i += 1;
    }
    if at == usize::MAX || at == 0 || at + 1 >= b.len() {
        return false;
    }
    // The domain needs a dot, and it may sit neither first nor last.
    let mut dot = usize::MAX;
    let mut j = at + 1;
    while j < b.len() {
        if b[j] == b'.' {
            dot = j;
        }
        j += 1;
    }
    dot != usize::MAX && dot > at + 1 && dot + 1 < b.len()
}

const fn is_uuid(b: &[u8]) -> bool {
    if b.len() != 36 {
        return false;
    }
    let mut i = 0;
    while i < 36 {
        let hyphen = i == 8 || i == 13 || i == 18 || i == 23;
        if hyphen {
            if b[i] != b'-' {
                return false;
            }
        } else if !is_hex(b[i]) {
            return false;
        }
        i += 1;
    }
    true
}

const fn is_date(b: &[u8]) -> bool {
    if b.len() != 10 {
        return false;
    }
    if b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let mut i = 0;
    while i < 10 {
        if i != 4 && i != 7 && !is_digit(b[i]) {
            return false;
        }
        i += 1;
    }
    let month = (b[5] - b'0') as u32 * 10 + (b[6] - b'0') as u32;
    let day = (b[8] - b'0') as u32 * 10 + (b[9] - b'0') as u32;
    month >= 1 && month <= 12 && day >= 1 && day <= 31
}
