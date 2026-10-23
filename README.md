# zod

Schemas a literal must satisfy while the crate compiles, and input must
satisfy while it runs. One rule, written once, applied in both places.

```rust
use zod::{Int, Text};

const HANDLE: Text = Text::new().min(3).max(16).slug();
const PORT: Int = Int::new().from(1).to(65535);

zod::accepts!(HANDLE, "ada-lovelace");   // the compiler runs the rule
zod::accepts!(PORT, 8080);
```

Change `"ada-lovelace"` to `"A"` and the build stops:

```
error[E0080]: evaluation panicked: too short
 --> src/main.rs:6:1
  |
6 | zod::accepts!(HANDLE, "A");
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^ evaluation of `_` failed inside this call
```

The same `HANDLE` reads a form field, a JSON body or a query string at run
time, and reports every fault with the path that locates it.

## Is there already something?

Yes, and none of it is this. Figures read from crates.io on 23 October 2026.

| Crate | Recent downloads | What it does | What it does not |
|---|---|---|---|
| `serde` + `serde_json` | the ecosystem | parses untrusted bytes into your types | knows nothing narrower than a type |
| `validator` | 14.9M | `#[validate(email, length(min = 1))]` on a struct | runs at run time only; the rule is an attribute, not a value |
| `garde` | 2.7M | the same idea, with validation contexts | the same limit |
| `nutype` | 1.7M | a newtype that cannot hold a bad value | the rule is a macro argument; you cannot hold it, pass it or reuse it |
| `schemars` / `jsonschema` | 215M / 33M | JSON Schema out of, and against, Rust types | a schema document, not a Rust rule |

The crates that advertise themselves as Zod for Rust — `zod-rs`, `vld`,
`domainstack`, `typeshift` — are all first published in 2025 or 2026 and all
below 5,000 downloads in total. They are ports, and a port is the wrong
shape, for one reason:

**Zod exists because TypeScript throws its types away.** `z.infer` recovers a
static type from a runtime schema because there is no other way to get one.
Rust never threw the type away. A port spends its effort on the half the
language already solved, and leaves the half it did not.

## What Rust does not have

A rule narrower than a type, held as a value, that the compiler can run.

`validator` and `garde` cannot: an attribute is not a value, so no function
takes one as an argument. `nutype` cannot, for the same reason. None of them
can look at a literal in your source and refuse it before the program runs,
although every rule they express is decidable right there.

This crate is that value. A `Text` or an `Int` is an ordinary `const` you can
store, pass and compose, and every method on it is a `const fn`.

## The two questions

Libraries confuse these. They are different questions with different answers.

- **parse**: *is this unknown input a valid `T`?* It converts. It can fail on
  the type before it ever reaches a rule.
- **check**: *is this `T`, which I already hold, one the rules accept?* No
  conversion. The compiler settled the type already.

```rust
let schema = object((("name", NAME), ("age", AGE)));

schema.parse(Value::from_form("name=Ada&age=36"))?;  // from the wire
schema.check(&("Ada".to_string(), 36))               // from your own code
```

A caller that built the value itself has already settled the first question.
Re-parsing it throws away the proof and converts a value that needs no
conversion. The invariant that holds the two together, and that the tests
enforce: `check(v).is_empty()` agrees with whether `parse` would accept the
input that produces `v`.

## Values that cannot be wrong

```rust
zod::newtype!(
    /// A public name, lowercase and hyphenated.
    pub struct Handle(text) = Text::new().min(3).max(16).slug()
);

const ADA: Handle = Handle::lit("ada-lovelace");  // checked by the compiler
let found = Handle::new(input)?;                  // checked at run time
```

`lit` is a `const fn`. Bound to a `const`, a bad literal is a build failure
and the good one costs no allocation — it borrows the `&'static str`. `new`
runs the same rules against something the program found, and returns every
fault.

This is `nutype`'s guarantee, plus the compile-time half, plus a rule you can
still reach: `Handle::RULES` is a value.

## Input

```rust
Value::from_form("name=Ada+Lovelace&age=36&subscribed=on")
Value::from_query("?page=2")
Value::from(json)   // with the "json" feature
```

A form carries no types, so every field arrives as text and a schema converts
it where it says to: `text_int`, `text_real`, `text_bool`. Conversion is never
silent. A JSON body that sends `"36"` for a number is a bug in the sender, and
a schema that hides it makes the bug permanent.

Form rules worth knowing: a field with no value is nothing, not empty text; a
repeated name becomes a list, which is what a group of checkboxes sends; an
unticked checkbox sends nothing at all, and `text_bool` reads that as false.

## Faults

Every fault, never only the first, each with the path that finds it.

```rust
let faults = schema.parse(body).unwrap_err();

faults.to_string();
// 2 faults:
//   name: too short (1, found 0)
//   people.0.age: above the maximum (130, found 200)

faults.by_path();   // [("name", ["too short (1, found 0)"]), ...]
```

A `Flaw` carries `code`, `says`, `limit` and `found` as separate fields, so a
caller can write its own message in its own language without parsing a string.

## What is here

- `Text` — `min` `max` `len` `filled` `starts` `ends` `holds`, and the shapes
  `ascii` `alnum` `digits` `hex` `lower` `upper` `slug` `email` `uuid` `date`.
  Every one of them `const`.
- `Int` — `from` `to` `positive` `negative` `unsigned` `step`.
- `Real` — `from` `to` `positive` `wild`. Finite by default, because `NaN`
  passing a range check is a bug every time.
- `object(...)` up to twelve fields, `strict()`, `list()`, `optional()`,
  `or(default)`, `refine(says, test)`.
- `Value::from_form`, `from_query`, and `From<serde_json::Value>`.

43 tests, no dependency outside the standard library unless you turn on
`json`.

## What is not here yet

- **No regular expression.** A regex engine cannot run inside the compiler,
  and a rule that works only at run time does not belong in the const layer.
  Use `refine` for now.
- **No derive.** `object(...)` returns a tuple. A `#[derive(Check)]` that puts
  the rules on a real struct's fields is the next piece, and needs a
  proc-macro crate beside this one.
- **No unions, no maps with dynamic keys, no transforms.** A transform
  changes the type, which makes `check` ill-defined; it needs a design, not
  an afternoon.
- **`u128` above `i128::MAX`** does not fit the comparison and is refused
  rather than wrapped.

## Why it is called zod

Because that is the shape people recognise. It is not a port. See above.

## Licence

MIT.
