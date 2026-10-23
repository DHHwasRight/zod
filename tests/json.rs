//! A JSON body is one more source of unknown input.

#![cfg(feature = "json")]

use zod::{list, object, Input, Int, Schema, Text};

#[test]
fn a_json_body_becomes_input_a_schema_can_read() {
    let body: serde_json::Value = serde_json::from_str(
        r#"{"name": "Ada", "age": 36, "tags": ["maths", "engines"], "extra": null}"#,
    )
    .unwrap();

    let schema = object((
        ("name", Text::new().min(1)),
        ("age", Int::new().from(0).to(130)),
        ("tags", list(Text::new().min(2)).min(1)),
    ));

    let (name, age, tags) = schema.parse(Input::json(body)).unwrap();
    assert_eq!(name, "Ada");
    assert_eq!(age, 36);
    assert_eq!(tags, vec!["maths", "engines"]);
}

#[test]
fn json_null_is_nothing_and_an_absent_field_is_the_same() {
    let schema = object((("name", Text::new().optional()),));

    let explicit: serde_json::Value = serde_json::from_str(r#"{"name": null}"#).unwrap();
    let absent: serde_json::Value = serde_json::from_str("{}").unwrap();

    assert_eq!(schema.parse(Input::json(explicit)).unwrap().0, None);
    assert_eq!(schema.parse(Input::json(absent)).unwrap().0, None);
}

#[test]
fn a_json_string_is_not_quietly_read_as_a_number() {
    let body: serde_json::Value = serde_json::from_str(r#"{"age": "36"}"#).unwrap();
    let schema = object((("age", Int::new()),));

    let faults = schema.parse(Input::json(body)).unwrap_err();
    assert_eq!(faults.all()[0].flaw.code, zod::Code::WrongType);
}

#[test]
fn every_fault_in_a_json_body_comes_back_with_its_path() {
    let body: serde_json::Value =
        serde_json::from_str(r#"{"name": "", "age": 200, "tags": ["x"]}"#).unwrap();

    let schema = object((
        ("name", Text::new().min(1)),
        ("age", Int::new().from(0).to(130)),
        ("tags", list(Text::new().min(2))),
    ));

    let faults = schema.parse(Input::json(body)).unwrap_err();
    let paths: Vec<String> = faults.all().iter().map(|f| f.path.to_string()).collect();
    assert_eq!(paths, vec!["name", "age", "tags.0"]);
}
