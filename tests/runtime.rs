//! Reading input nobody has proven yet, and checking values the program
//! already holds.

use zod::{boolean, list, object, Input, Int, Real, Schema, Text, Value};

const NAME: Text = Text::new().min(1).max(40);
const AGE: Int = Int::new().from(0).to(130);

#[test]
fn a_schema_reads_an_object_into_a_tuple() {
    let person = object((("name", NAME), ("age", AGE)));

    let input = Value::Map(vec![
        ("name".into(), Value::Text("Ada".into())),
        ("age".into(), Value::Int(36)),
    ]);

    let (name, age) = person.parse(input).unwrap();
    assert_eq!(name, "Ada");
    assert_eq!(age, 36);
}

#[test]
fn every_bad_field_comes_back_at_once() {
    let person = object((("name", NAME), ("age", AGE)));

    let input = Value::Map(vec![
        ("name".into(), Value::Text(String::new())),
        ("age".into(), Value::Int(-1)),
    ]);

    let faults = person.parse(input).unwrap_err();
    assert_eq!(faults.len(), 2, "{faults}");
    assert_eq!(faults.all()[0].path.to_string(), "name");
    assert_eq!(faults.all()[1].path.to_string(), "age");
}

#[test]
fn a_fault_inside_a_list_keeps_the_index_that_finds_it() {
    let schema = list(Text::new().min(3));
    let input = Value::List(vec!["ok!".into(), "no".into()]);

    let faults = schema.parse(input).unwrap_err();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults.all()[0].path.to_string(), "1");
}

#[test]
fn a_path_reads_from_the_outside_in() {
    let schema = object((("people", list(object((("name", NAME),)))),));
    let input = Value::Map(vec![(
        "people".into(),
        Value::List(vec![Value::Map(vec![(
            "name".into(),
            Value::Text(String::new()),
        )])]),
    )]);

    let faults = schema.parse(input).unwrap_err();
    assert_eq!(faults.all()[0].path.to_string(), "people.0.name");
}

#[test]
fn an_unknown_field_is_dropped_unless_the_schema_is_strict() {
    let loose = object((("name", NAME),));
    let strict = object((("name", NAME),)).strict();

    let input = || {
        Value::Map(vec![
            ("name".into(), Value::Text("Ada".into())),
            ("colour".into(), Value::Text("green".into())),
        ])
    };

    assert!(loose.parse(input()).is_ok());
    let faults = strict.parse(input()).unwrap_err();
    assert_eq!(faults.all()[0].path.to_string(), "colour");
    assert_eq!(faults.all()[0].flaw.code, zod::Code::Unexpected);
}

#[test]
fn nothing_is_a_fault_unless_the_field_is_optional() {
    let needed = object((("name", NAME),));
    let spare = object((("name", NAME.optional()),));

    let empty = || Value::Map(Vec::new());

    assert!(needed.parse(empty()).is_err());
    assert_eq!(spare.parse(empty()).unwrap().0, None);
}

#[test]
fn a_fallback_goes_through_the_rules_like_any_other_value() {
    let good = AGE.or(18);
    assert_eq!(good.parse(Value::Null).unwrap(), 18);
    assert_eq!(good.parse(Value::Int(40)).unwrap(), 40);

    // A default the schema would refuse is a fault, not a surprise.
    let bad = AGE.or(-5);
    assert!(bad.parse(Value::Null).is_err());
}

#[test]
fn a_rule_the_caller_wrote_runs_in_both_directions() {
    let even = Int::new().refine("must be even", |n| n % 2 == 0);

    assert!(even.parse(Value::Int(4)).is_ok());
    assert!(even.parse(Value::Int(5)).is_err());
    assert!(even.accepts(&4));
    assert!(!even.accepts(&5));
}

#[test]
fn the_wrong_type_stops_before_the_rules_run() {
    let faults = NAME.parse(Value::Int(3)).unwrap_err();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults.all()[0].flaw.code, zod::Code::WrongType);
}

#[test]
fn check_asks_the_other_question_and_needs_no_conversion() {
    let person = object((("name", NAME), ("age", AGE)));
    let held = ("Ada".to_string(), 36i64);

    assert!(person.check(&held).is_empty());

    let wrong = (String::new(), 200i64);
    let faults = person.check(&wrong);
    assert_eq!(faults.len(), 2);
    assert_eq!(faults.all()[0].path.to_string(), "name");
    assert_eq!(faults.all()[1].path.to_string(), "age");
}

#[test]
fn check_and_parse_cannot_disagree() {
    let schema = object((
        ("name", NAME),
        ("age", AGE),
        ("tags", list(Text::new().min(2)).max(3)),
    ));

    let cases: Vec<(String, i64, Vec<String>)> = vec![
        ("Ada".into(), 36, vec!["one".into()]),
        ("".into(), 36, vec!["one".into()]),
        ("Ada".into(), 999, vec!["one".into()]),
        ("Ada".into(), 36, vec!["x".into()]),
        (
            "Ada".into(),
            36,
            vec!["aa".into(), "bb".into(), "cc".into(), "dd".into()],
        ),
        ("".into(), -1, vec!["x".into()]),
    ];

    for held in cases {
        let input = Value::Map(vec![
            ("name".into(), Value::Text(held.0.clone())),
            ("age".into(), Value::Int(held.1)),
            (
                "tags".into(),
                Value::List(held.2.iter().cloned().map(Value::Text).collect()),
            ),
        ]);

        let parsed = schema.parse(input).is_ok();
        let checked = schema.check(&held).is_empty();
        assert_eq!(parsed, checked, "parse and check disagree about {held:?}");
    }
}

#[test]
fn a_form_arrives_as_text_and_a_schema_converts_it_on_purpose() {
    let form = Input::form("name=Ada+Lovelace&age=36&subscribed=on");
    let schema = object((("name", NAME), ("age", AGE), ("subscribed", boolean())));

    let (name, age, subscribed) = schema.parse(form).unwrap();
    assert_eq!(name, "Ada Lovelace");
    assert_eq!(age, 36);
    assert!(subscribed);
}

#[test]
fn an_unticked_checkbox_sends_nothing_and_means_false() {
    let form = Input::form("name=Ada");
    let schema = object((("name", NAME), ("subscribed", boolean())));

    let (_, subscribed) = schema.parse(form).unwrap();
    assert!(!subscribed);
}

#[test]
fn a_form_decodes_percent_escapes_and_plus() {
    let form = Input::form("who=Ada%20Lovelace&what=caf%C3%A9+au+lait");
    let schema = object((("who", Text::new()), ("what", Text::new())));

    let (who, what) = schema.parse(form).unwrap();
    assert_eq!(who, "Ada Lovelace");
    assert_eq!(what, "café au lait");
}

#[test]
fn a_repeated_form_field_becomes_a_list() {
    let form = Input::form("tag=one&tag=two&tag=three");
    let schema = object((("tag", list(Text::new()).min(1)),));

    let (tags,) = schema.parse(form).unwrap();
    assert_eq!(tags, vec!["one", "two", "three"]);
}

#[test]
fn an_empty_form_field_is_nothing_not_empty_text() {
    let form = Input::form("name=&age=36");
    let schema = object((("name", NAME.optional()), ("age", AGE)));

    let (name, age) = schema.parse(form).unwrap();
    assert_eq!(name, None);
    assert_eq!(age, 36);
}

#[test]
fn text_that_is_not_a_number_says_so_rather_than_panicking() {
    let form = Input::form("age=thirty");
    let schema = object((("age", AGE),));

    let faults = schema.parse(form).unwrap_err();
    assert_eq!(faults.all()[0].path.to_string(), "age");
    assert_eq!(faults.all()[0].flaw.says, "not a whole number");
}

#[test]
fn a_query_string_reads_like_a_form_with_or_without_the_question_mark() {
    assert_eq!(Input::query("?a=1"), Input::form("a=1"));
}

#[test]
fn one_schema_reads_a_form_and_a_typed_body_without_changing() {
    let schema = object((("name", NAME), ("age", AGE)));

    let from_form = schema.parse(Input::form("name=Ada&age=36")).unwrap();
    let from_typed = schema
        .parse(Value::Map(vec![
            ("name".into(), Value::Text("Ada".into())),
            ("age".into(), Value::Int(36)),
        ]))
        .unwrap();

    assert_eq!(from_form, from_typed);
}

#[test]
fn typed_input_does_not_read_text_as_a_number() {
    let schema = object((("age", AGE),));
    let typed = Value::Map(vec![("age".into(), Value::Text("36".into()))]);

    let faults = schema.parse(typed.clone()).unwrap_err();
    assert_eq!(faults.all()[0].flaw.code, zod::Code::WrongType);

    // Unless the schema says to, for an API that must keep accepting it.
    let lenient = object((("age", AGE.loose()),));
    assert_eq!(lenient.parse(typed).unwrap().0, 36);
}

#[test]
fn an_absent_field_says_it_is_missing_not_that_it_is_the_wrong_type() {
    let schema = object((("name", NAME),));
    let faults = schema.parse(Value::Map(Vec::new())).unwrap_err();
    assert_eq!(faults.all()[0].flaw.code, zod::Code::Missing);
}

#[test]
fn a_form_sends_one_value_for_a_list_of_one() {
    let schema = object((("tag", list(Text::new()).min(1)),));
    let (tags,) = schema.parse(Input::form("tag=only")).unwrap();
    assert_eq!(tags, vec!["only"]);
}

#[test]
fn faults_group_by_field_for_a_form_that_shows_them_next_to_the_input() {
    let schema = object((("name", NAME), ("age", AGE)));
    let input = Value::Map(vec![
        ("name".into(), Value::Text(String::new())),
        ("age".into(), Value::Int(-1)),
    ]);

    let faults = schema.parse(input).unwrap_err();
    let grouped = faults.by_path();
    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped[0].0, "name");
    assert_eq!(grouped[0].1.len(), 1);
}

#[test]
fn a_decimal_accepts_a_whole_number_because_json_writes_one_point_zero_as_one() {
    let schema = Real::new().from(0.0).to(1.0);
    assert_eq!(schema.parse(Value::Int(1)).unwrap(), 1.0);
}

#[test]
fn a_message_names_the_bound_and_what_arrived() {
    let faults = AGE.parse(Value::Int(200)).unwrap_err();
    assert_eq!(faults.to_string(), "above the maximum (130, found 200)");
}
