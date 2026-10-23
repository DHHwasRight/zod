//! The second way to say what a field allows: an annotation rather than a
//! value, and a callback the struct carries.

use zod::{Entry, Input, Node, Schema, Text, Value};

#[derive(Entry, Debug, PartialEq)]
struct Signup {
    #[text(min = 3, max = 16, slug)]
    #[label("Handle")]
    handle: String,

    #[text(filled, email)]
    email: String,

    #[int(min = 13, max = 130)]
    age: i64,

    #[int(min = 1, max = 99)]
    quantity: u8,

    #[real(min = 0.0, max = 1.0)]
    ratio: f64,

    #[text(min = 2)]
    #[list(min = 1, max = 3)]
    tags: Vec<String>,

    #[text(min = 4)]
    note: Option<String>,

    subscribed: bool,
}

fn sent(handle: &str, age: i64) -> Input {
    Input::typed(Value::Map(vec![
        ("handle".into(), Value::Text(handle.into())),
        ("email".into(), Value::Text("ada@example.com".into())),
        ("age".into(), Value::Int(age)),
        ("quantity".into(), Value::Int(2)),
        ("ratio".into(), Value::Real(0.5)),
        ("tags".into(), Value::List(vec!["one".into()])),
        ("subscribed".into(), Value::Bool(true)),
    ]))
}

#[test]
fn an_annotation_builds_the_same_schema_a_value_would() {
    let signup = Signup::parse(sent("ada-lovelace", 36)).unwrap();
    assert_eq!(signup.handle, "ada-lovelace");
    assert_eq!(signup.quantity, 2);
    assert_eq!(signup.tags, vec!["one"]);
    assert_eq!(signup.note, None);
}

#[test]
fn each_annotated_rule_still_refuses_what_it_should() {
    assert!(
        Signup::parse(sent("A", 36)).is_err(),
        "too short, and not a slug"
    );
    assert!(
        Signup::parse(sent("ada-lovelace", 9)).is_err(),
        "below the minimum"
    );
}

#[test]
fn a_narrow_integer_keeps_its_own_range_under_an_annotation() {
    let mut input = sent("ada-lovelace", 36);
    if let Value::Map(fields) = &mut input.value {
        fields[3].1 = Value::Int(300);
    }
    let faults = Signup::parse(input).unwrap_err();
    assert_eq!(faults.all()[0].path.to_string(), "quantity");
}

#[test]
fn an_option_field_is_optional_without_saying_so_twice() {
    let mut input = sent("ada-lovelace", 36);
    if let Value::Map(fields) = &mut input.value {
        fields.push(("note".into(), Value::Text("ab".into())));
    }
    let faults = Signup::parse(input).unwrap_err();
    assert_eq!(
        faults.all()[0].path.to_string(),
        "note",
        "present, so the rule runs"
    );
}

#[test]
fn a_vec_field_becomes_a_list_and_the_annotation_describes_one_item() {
    let mut input = sent("ada-lovelace", 36);
    if let Value::Map(fields) = &mut input.value {
        fields[5].1 = Value::List(vec!["x".into()]);
    }
    let faults = Signup::parse(input).unwrap_err();
    assert_eq!(faults.all()[0].path.to_string(), "tags.0", "the item rule");

    let mut input = sent("ada-lovelace", 36);
    if let Value::Map(fields) = &mut input.value {
        fields[5].1 = Value::List(Vec::new());
    }
    let faults = Signup::parse(input).unwrap_err();
    assert_eq!(faults.all()[0].path.to_string(), "tags", "the count rule");
}

// -- a callback the struct carries ----------------------------------------

#[derive(Entry, Debug)]
#[entry(
    rule = passwords_agree,
    says = "The two passwords are not the same",
    rule = not_the_handle,
    says = "The password may not be the handle"
)]
struct Password {
    #[text(min = 3)]
    handle: String,
    #[text(min = 8)]
    password: String,
    #[text(min = 8)]
    again: String,
}

fn passwords_agree(it: &Password) -> bool {
    it.password == it.again
}

fn not_the_handle(it: &Password) -> bool {
    it.password != it.handle
}

fn three(password: &str, again: &str) -> Input {
    Input::typed(Value::Map(vec![
        ("handle".into(), Value::Text("ada-lovelace".into())),
        ("password".into(), Value::Text(password.into())),
        ("again".into(), Value::Text(again.into())),
    ]))
}

#[test]
fn a_rule_on_the_struct_sees_every_field_at_once() {
    assert!(Password::parse(three("correct horse", "correct horse")).is_ok());

    let faults = Password::parse(three("correct horse", "battery staple")).unwrap_err();
    assert_eq!(faults.len(), 1);
    assert_eq!(
        faults.all()[0].flaw.says,
        "The two passwords are not the same"
    );
    assert_eq!(faults.all()[0].flaw.code, zod::Code::Custom);
}

#[test]
fn every_struct_rule_is_reported_not_only_the_first() {
    let faults = Password::parse(three("ada-lovelace", "something else")).unwrap_err();
    assert_eq!(faults.len(), 2, "{faults}");
}

#[test]
fn a_struct_rule_does_not_run_while_a_field_is_still_wrong() {
    // Both passwords are too short, so comparing them would be comparing
    // two things the schema has already refused. The two field faults come
    // back and the struct rule stays quiet.
    let faults = Password::parse(three("short", "short")).unwrap_err();
    let paths: Vec<String> = faults.all().iter().map(|f| f.path.to_string()).collect();
    assert_eq!(paths, vec!["password", "again"]);
    assert!(
        faults
            .all()
            .iter()
            .all(|f| f.flaw.code != zod::Code::Custom),
        "no struct rule ran: {faults}"
    );
}

#[test]
fn check_runs_the_struct_rule_too() {
    let held = Password {
        handle: "ada-lovelace".into(),
        password: "correct horse".into(),
        again: "battery staple".into(),
    };
    assert_eq!(held.check().len(), 1);
    assert!(!held.sound());
}

// -- a refusal the struct carries -----------------------------------------

zod::refusal! {
    /// One list item per field, for a form that draws itself again.
    pub Listing(faults) -> String {
        faults
            .by_name()
            .into_iter()
            .map(|(name, says)| format!("<li>{name}: {}</li>", says.join("; ")))
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Entry, Debug)]
#[entry(refuse = Listing)]
struct Small {
    #[text(min = 3)]
    #[label("Your name")]
    name: String,
}

#[test]
fn a_named_callback_is_what_the_entrance_gives_back() {
    let page = Small::enter(Input::form("name=A")).unwrap_err();
    assert_eq!(page, "<li>Your name: too short (3, found 1)</li>");
}

#[test]
fn the_callback_is_a_refusal_like_any_other() {
    let faults = Small::parse(Input::form("name=A")).unwrap_err();
    assert_eq!(
        zod::Refusal::refuse(&Listing, &faults),
        Small::enter(Input::form("name=A")).unwrap_err()
    );
}

// -- what the schema says about itself ------------------------------------

#[test]
fn a_struct_describes_itself_under_the_name_it_has() {
    let node = Signup::describe();
    assert_eq!(node.name(), Some("Signup"));

    let object = node.object().unwrap();
    let names: Vec<&str> = object.fields.iter().map(|f| f.name).collect();
    assert_eq!(
        names,
        vec![
            "handle",
            "email",
            "age",
            "quantity",
            "ratio",
            "tags",
            "note",
            "subscribed"
        ]
    );
}

#[test]
fn the_description_carries_the_bounds_the_annotation_set() {
    let node = Signup::describe();
    let object = node.object().unwrap();

    let Node::Text(handle) = object.fields[0].node.bare() else {
        panic!("handle is text");
    };
    assert_eq!(handle.min, 3);
    assert_eq!(handle.max, 16);
    assert_eq!(handle.shape, zod::TextShape::Slug);

    let Node::Int(age) = object.fields[2].node.bare() else {
        panic!("age is a whole number");
    };
    assert_eq!((age.min, age.max), (13, 130));
}

#[test]
fn a_narrow_integer_describes_the_range_it_actually_accepts() {
    let node = Signup::describe();
    let Node::Int(quantity) = node.object().unwrap().fields[3].node.bare() else {
        panic!("quantity is a whole number");
    };
    assert_eq!((quantity.min, quantity.max), (1, 99));

    // With no rule at all, the type is still the rule.
    #[derive(Entry, Debug)]
    struct Narrow {
        small: u8,
    }
    let described = Narrow::describe();
    let Node::Int(small) = described.object().unwrap().fields[0].node.bare() else {
        panic!("small is a whole number");
    };
    assert_eq!((small.min, small.max), (0, 255));
}

#[test]
fn required_is_derived_from_the_schema_and_not_declared_twice() {
    let node = Signup::describe();
    let object = node.object().unwrap();
    assert!(object.fields[0].required, "handle");
    assert!(!object.fields[6].required, "note is an Option");
}

#[test]
fn a_label_and_a_message_belong_to_the_field_in_a_description() {
    let node = Signup::describe();
    assert_eq!(node.object().unwrap().fields[0].label, Some("Handle"));

    #[derive(Entry, Debug)]
    struct Said {
        #[text(min = 3)]
        #[says("Pick three letters or more")]
        handle: String,
    }
    let node = Said::describe();
    assert_eq!(
        node.object().unwrap().fields[0].says,
        Some("Pick three letters or more")
    );
}

#[test]
fn a_list_describes_its_counts_and_its_item() {
    let node = Signup::describe();
    let Node::List { item, min, max } = node.object().unwrap().fields[5].node.bare() else {
        panic!("tags is a list");
    };
    assert_eq!((*min, *max), (1, 3));
    let Node::Text(tag) = item.bare() else {
        panic!("a tag is text")
    };
    assert_eq!(tag.min, 2);
}

#[test]
fn a_nested_struct_is_named_in_the_description_so_a_document_can_refer_to_it() {
    #[derive(Entry, Debug)]
    struct Address {
        #[text(filled)]
        street: String,
    }

    #[derive(Entry, Debug)]
    struct Order {
        #[nested]
        ship_to: Address,
        #[check(zod::list(zod::nested::<Address>()))]
        also: Vec<Address>,
    }

    let node = Order::describe();
    let names: Vec<&str> = node.named_parts().iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        vec!["Order", "Address"],
        "each one once, outermost first"
    );
}

#[test]
fn a_schema_built_by_hand_describes_itself_too() {
    let schema = zod::object((
        ("name", Text::new().min(1).label("Your name")),
        ("tags", zod::list(Text::new()).max(3)),
    ));
    let node = schema.node();
    let object = node.object().unwrap();
    assert_eq!(object.fields[0].label, Some("Your name"));
    assert_eq!(object.fields.len(), 2);
}
