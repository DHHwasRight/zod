//! What the compiler proves before the program runs.

use zod::{Int, Real, Text};

const HANDLE: Text = Text::new().min(3).max(16).slug();
const EMAIL: Text = Text::new().filled().email();
const PORT: Int = Int::new().from(1).to(65535);
const RATIO: Real = Real::new().from(0.0).to(1.0);

// Each of these is evaluated while this file compiles. A wrong literal is a
// build failure, and the test below only confirms the rule still holds at
// run time.
zod::accepts!(HANDLE, "ada-lovelace");
zod::accepts!(EMAIL, "ada@example.com");
zod::accepts!(PORT, 8080);
zod::accepts!(RATIO, 0.5);

zod::newtype!(
    /// A public name.
    pub struct Handle(text) = Text::new().min(3).max(16).slug()
);
zod::newtype!(
    /// A TCP port.
    pub struct Port(int) = Int::new().from(1).to(65535)
);

const ADA: Handle = Handle::lit("ada-lovelace");
const HTTPS: Port = Port::lit(443);

#[test]
fn a_literal_the_compiler_accepted_is_still_good_at_run_time() {
    assert!(HANDLE.accepts("ada-lovelace"));
    assert!(EMAIL.accepts("ada@example.com"));
    assert!(PORT.accepts(8080));
    assert!(RATIO.accepts(0.5));
}

#[test]
fn a_const_newtype_costs_no_allocation() {
    assert_eq!(ADA.as_str(), "ada-lovelace");
    assert_eq!(HTTPS.get(), 443);
}

#[test]
fn the_same_rules_judge_a_value_found_at_run_time() {
    assert!(Handle::new("ada-lovelace").is_ok());

    let faults = Handle::new("A").unwrap_err();
    assert_eq!(faults.len(), 2, "too short, and not a slug: {faults}");

    assert!(Port::new(0).is_err());
    assert!(Port::new(443).is_ok());
}

#[test]
fn a_newtype_parses_from_a_string() {
    let handle: Handle = "ada-lovelace".parse().unwrap();
    assert_eq!(handle.to_string(), "ada-lovelace");
    assert!("Ada Lovelace".parse::<Handle>().is_err());
}

#[test]
fn text_counts_characters_not_bytes() {
    const FIVE: Text = Text::new().len(5);
    assert!(FIVE.accepts("héllo"));
    assert!(!FIVE.accepts("hello!"));
}

#[test]
fn a_shape_names_itself_in_the_fault() {
    let flaw = Text::new().uuid().check("not-a-uuid").unwrap();
    assert_eq!(flaw.says, "a UUID");
    assert_eq!(flaw.code, zod::Code::BadShape);
}

#[test]
fn an_email_rule_refuses_what_people_actually_mistype() {
    const E: Text = Text::new().email();
    assert!(E.accepts("a@b.co"));
    assert!(!E.accepts("a@b"), "no dot in the domain");
    assert!(!E.accepts("@b.co"), "no local part");
    assert!(!E.accepts("a@@b.co"), "two at signs");
    assert!(!E.accepts("a b@c.co"), "a space");
    assert!(!E.accepts("a@.co"), "the dot opens the domain");
}

#[test]
fn a_date_rule_refuses_a_month_that_does_not_exist() {
    const D: Text = Text::new().date();
    assert!(D.accepts("2026-10-23"));
    assert!(!D.accepts("2026-13-01"));
    assert!(!D.accepts("2026-00-01"));
    assert!(!D.accepts("2026-1-1"));
}

#[test]
fn a_slug_refuses_hyphens_at_the_edges_and_doubled() {
    const S: Text = Text::new().slug();
    assert!(S.accepts("one-two-3"));
    assert!(!S.accepts("-one"));
    assert!(!S.accepts("one-"));
    assert!(!S.accepts("one--two"));
    assert!(!S.accepts("One"));
}

#[test]
fn a_step_and_a_bound_are_separate_faults() {
    const EVEN_SMALL: Int = Int::new().to(10).step(2);
    let faults = EVEN_SMALL.faults(11);
    assert_eq!(faults.len(), 2);
}

#[test]
fn a_decimal_refuses_nan_before_it_compares_anything() {
    const R: Real = Real::new().from(0.0).to(1.0);
    assert!(!R.accepts(f64::NAN));
    assert_eq!(R.check(f64::NAN).unwrap().code, zod::Code::NotFinite);
    assert!(Real::new().wild().accepts(f64::NAN));
}
