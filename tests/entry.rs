//! A struct that guards an entrance.

use zod::{Entry, Faults, Input, Int, Problem, Refusal, Schema, Text, Value};

const HANDLE: Text = Text::new().min(3).max(16).slug();
const EMAIL: Text = Text::new().filled().email();
const AGE: Int = Int::new().from(13).to(130);

#[derive(Entry, Clone, PartialEq, Debug)]
pub struct Signup {
    #[check(HANDLE)]
    #[label("Handle")]
    #[says("Pick 3 to 16 lowercase letters, digits or hyphens")]
    pub handle: String,

    #[check(EMAIL)]
    #[label("Email address")]
    pub email: String,

    #[check(AGE)]
    pub age: i64,

    pub subscribed: bool,

    #[name("phone_number")]
    pub phone: Option<String>,
}

#[derive(Entry, Clone, PartialEq, Debug)]
pub struct Address {
    #[check(Text::new().filled())]
    pub street: String,

    #[check(Text::new().len(5).digits())]
    #[label("Post code")]
    pub postcode: String,
}

#[derive(Entry, Clone, PartialEq, Debug)]
pub struct Order {
    #[check(Text::new().uuid())]
    pub reference: String,

    #[nested]
    pub ship_to: Address,

    #[check(zod::list(zod::nested::<Address>()).max(3))]
    pub also: Vec<Address>,

    #[check(Int::new().from(1).to(99).of::<u8>())]
    pub quantity: u8,
}

#[test]
fn the_struct_is_the_schema() {
    let signup = Signup::parse(Input::form(
        "handle=ada-lovelace&email=ada@example.com&age=36&subscribed=on",
    ))
    .unwrap();

    assert_eq!(signup.handle, "ada-lovelace");
    assert_eq!(signup.email, "ada@example.com");
    assert_eq!(signup.age, 36);
    assert!(signup.subscribed);
    assert_eq!(signup.phone, None);
}

#[test]
fn one_struct_reads_a_form_and_a_json_body_the_same_way() {
    let from_form = Signup::parse(Input::form(
        "handle=ada-lovelace&email=ada@example.com&age=36",
    ))
    .unwrap();

    let from_typed = Signup::parse(Value::Map(vec![
        ("handle".into(), Value::Text("ada-lovelace".into())),
        ("email".into(), Value::Text("ada@example.com".into())),
        ("age".into(), Value::Int(36)),
        ("subscribed".into(), Value::Bool(false)),
    ]))
    .unwrap();

    assert_eq!(from_form, from_typed);
}

#[test]
fn a_field_with_no_attribute_reads_by_its_own_type() {
    #[derive(Entry, Debug)]
    struct Plain {
        name: String,
        count: i64,
        ratio: f64,
        tags: Vec<String>,
        note: Option<String>,
    }

    let value = Plain::parse(Value::Map(vec![
        ("name".into(), Value::Text("x".into())),
        ("count".into(), Value::Int(2)),
        ("ratio".into(), Value::Real(0.5)),
        ("tags".into(), Value::List(vec!["a".into()])),
        ("note".into(), Value::Null),
    ]))
    .unwrap();

    assert_eq!(value.name, "x");
    assert_eq!(value.count, 2);
    assert_eq!(value.ratio, 0.5);
    assert_eq!(value.tags, vec!["a"]);
    assert_eq!(value.note, None);
}

#[test]
fn a_name_on_the_wire_may_differ_from_the_name_in_the_code() {
    let signup = Signup::parse(Input::form(
        "handle=ada-lovelace&email=ada@example.com&age=36&phone_number=555",
    ))
    .unwrap();

    assert_eq!(signup.phone.as_deref(), Some("555"));
}

#[test]
fn says_replaces_three_machine_messages_with_one_a_person_can_act_on() {
    let faults = Signup::parse(Input::form("handle=A&email=ada@example.com&age=36")).unwrap_err();

    let handle: Vec<String> = faults
        .at("handle")
        .iter()
        .map(|f| f.flaw.says.to_string())
        .collect();

    assert_eq!(handle.len(), 2, "two rules broke");
    assert!(handle
        .iter()
        .all(|s| s == "Pick 3 to 16 lowercase letters, digits or hyphens"));
}

#[test]
fn a_label_names_the_field_the_way_a_person_does() {
    let faults = Signup::parse(Input::form("handle=ada-lovelace&email=nope&age=36")).unwrap_err();

    assert_eq!(faults.all()[0].name(), "Email address");
    assert_eq!(faults.all()[0].path.to_string(), "email");
    assert_eq!(faults.by_name()[0].0, "Email address");
}

#[test]
fn a_nested_struct_keeps_the_path_that_finds_the_fault() {
    let order = Value::Map(vec![
        ("reference".into(), Value::Text("not-a-uuid".into())),
        (
            "ship_to".into(),
            Value::Map(vec![
                ("street".into(), Value::Text("1 Main".into())),
                ("postcode".into(), Value::Text("abcde".into())),
            ]),
        ),
        ("also".into(), Value::List(Vec::new())),
        ("quantity".into(), Value::Int(2)),
    ]);

    let faults = Order::parse(order).unwrap_err();
    let paths: Vec<String> = faults.all().iter().map(|f| f.path.to_string()).collect();

    assert_eq!(paths, vec!["reference", "ship_to.postcode"]);
    assert_eq!(faults.all()[1].name(), "Post code");
}

#[test]
fn a_list_of_nested_structs_keeps_the_index_too() {
    let order = Value::Map(vec![
        (
            "reference".into(),
            Value::Text("0191a3f0-0000-7000-8000-000000000000".into()),
        ),
        (
            "ship_to".into(),
            Value::Map(vec![
                ("street".into(), Value::Text("1 Main".into())),
                ("postcode".into(), Value::Text("75001".into())),
            ]),
        ),
        (
            "also".into(),
            Value::List(vec![Value::Map(vec![
                ("street".into(), Value::Text(String::new())),
                ("postcode".into(), Value::Text("75001".into())),
            ])]),
        ),
        ("quantity".into(), Value::Int(2)),
    ]);

    let faults = Order::parse(order).unwrap_err();
    assert_eq!(faults.all()[0].path.to_string(), "also.0.street");
}

#[test]
fn a_narrow_integer_refuses_a_number_that_would_not_survive_the_cast() {
    let order = |quantity: i64| {
        Value::Map(vec![
            (
                "reference".into(),
                Value::Text("0191a3f0-0000-7000-8000-000000000000".into()),
            ),
            (
                "ship_to".into(),
                Value::Map(vec![
                    ("street".into(), Value::Text("1 Main".into())),
                    ("postcode".into(), Value::Text("75001".into())),
                ]),
            ),
            ("also".into(), Value::List(Vec::new())),
            ("quantity".into(), Value::Int(quantity)),
        ])
    };

    assert_eq!(Order::parse(order(7)).unwrap().quantity, 7);

    let faults = Order::parse(order(300)).unwrap_err();
    assert_eq!(faults.all()[0].path.to_string(), "quantity");
}

#[test]
fn strict_refuses_a_field_the_struct_does_not_declare() {
    #[derive(Entry, Debug)]
    #[entry(strict)]
    struct Tight {
        name: String,
    }

    assert!(Tight::parse(Value::Map(vec![("name".into(), "x".into())])).is_ok());

    let faults = Tight::parse(Value::Map(vec![
        ("name".into(), "x".into()),
        ("colour".into(), "green".into()),
    ]))
    .unwrap_err();

    assert_eq!(faults.all()[0].path.to_string(), "colour");
    assert_eq!(faults.all()[0].flaw.code, zod::Code::Unexpected);
}

#[test]
fn check_runs_the_same_rules_against_a_struct_the_program_built() {
    let good = Signup {
        handle: "ada-lovelace".into(),
        email: "ada@example.com".into(),
        age: 36,
        subscribed: true,
        phone: None,
    };
    assert!(good.sound());

    let bad = Signup {
        handle: "A".into(),
        age: 200,
        ..good.clone()
    };
    let faults = bad.check();
    assert_eq!(
        faults.len(),
        3,
        "two on the handle, one on the age: {faults}"
    );
    assert_eq!(faults.at("age").len(), 1);
}

#[test]
fn check_and_parse_cannot_disagree_about_a_struct() {
    let cases = [
        ("ada-lovelace", "ada@example.com", 36),
        ("A", "ada@example.com", 36),
        ("ada-lovelace", "nope", 36),
        ("ada-lovelace", "ada@example.com", 200),
        ("", "", 0),
    ];

    for (handle, email, age) in cases {
        let held = Signup {
            handle: handle.into(),
            email: email.into(),
            age,
            subscribed: false,
            phone: None,
        };

        let input = Value::Map(vec![
            ("handle".into(), Value::Text(handle.into())),
            ("email".into(), Value::Text(email.into())),
            ("age".into(), Value::Int(age)),
            ("subscribed".into(), Value::Bool(false)),
        ]);

        assert_eq!(
            Signup::parse(input).is_ok(),
            held.check().is_empty(),
            "parse and check disagree about {held:?}"
        );
    }
}

// --- the refusal, which is the point of all of this -------------------

#[derive(Entry, Debug)]
#[entry(refuse = Problem)]
pub struct ApiSignup {
    #[check(HANDLE)]
    pub handle: String,
    #[check(AGE)]
    pub age: i64,
}

#[test]
fn an_entrance_carries_its_own_refusal() {
    let refused = ApiSignup::enter(Input::form("handle=A&age=200")).unwrap_err();

    assert_eq!(refused.status, 422);
    assert_eq!(refused.content_type, "application/problem+json");
    assert!(
        refused.body.contains("\"path\":\"handle\""),
        "{}",
        refused.body
    );
    assert!(
        refused.body.contains("\"code\":\"too_small\""),
        "{}",
        refused.body
    );
    assert!(
        refused.body.contains("\"limit\":130,\"found\":200"),
        "{}",
        refused.body
    );
}

#[test]
fn the_default_refusal_is_one_line_per_fault() {
    let message = Signup::enter(Input::form("handle=A&email=nope&age=9")).unwrap_err();
    assert!(message.starts_with("4 faults:"), "{message}");
    assert!(
        message.contains("Email address: an email address"),
        "{message}"
    );
}

#[test]
fn a_refusal_may_be_any_closure_that_builds_what_the_boundary_owes() {
    let page = |faults: &Faults| {
        let rows: Vec<String> = faults
            .by_name()
            .into_iter()
            .map(|(name, says)| format!("<li>{name}: {}</li>", says.join("; ")))
            .collect();
        format!("<ul>{}</ul>", rows.join(""))
    };

    let schema = zod::object((("name", Text::new().min(1).label("Your name")),));
    let html = schema
        .enter(Value::Map(vec![("name".into(), "".into())]), &page)
        .unwrap_err();

    assert_eq!(html, "<ul><li>Your name: too short (1, found 0)</li></ul>");
}

#[test]
fn a_refusal_can_group_the_messages_for_a_form_to_redraw_itself() {
    let faults = Signup::parse(Input::form("handle=A&email=nope&age=9")).unwrap_err();
    let fields = zod::Fields.refuse(&faults);

    let names: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, vec!["handle", "email", "age"]);
    assert_eq!(fields[0].1.len(), 2);
}

#[test]
fn the_problem_body_escapes_what_a_person_typed() {
    let schema = zod::object((("name", Text::new().min(99).says("a \"quoted\" rule")),));
    let refused = schema
        .enter(Value::Map(vec![("name".into(), "x".into())]), &Problem)
        .unwrap_err();

    assert!(
        refused.body.contains("a \\\"quoted\\\" rule"),
        "{}",
        refused.body
    );
}
