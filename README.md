# zod

One rule, written once, run by the compiler on your literals and by the
program on everything that arrives at its door.

```rust
use zod::{Entry, Input, Int, Problem, Text};

#[derive(Entry)]
#[entry(strict, refuse = Problem)]
struct Signup {
    #[check(Text::new().min(3).max(16).slug())]
    #[label("Handle")]
    #[says("Pick 3 to 16 lowercase letters, digits or hyphens")]
    handle: String,

    #[check(Int::new().from(13).to(130))]
    age: i64,

    subscribed: bool,
}

let signup = Signup::enter(Input::form(body))?;   // a form
let signup = Signup::enter(Input::json(body))?;   // a JSON body
let signup = Signup::enter(Input::env("APP_"))?;  // the environment
```

The error type of `enter` is whatever the refusal builds. With `Problem` it
is already an HTTP answer: status 422, `application/problem+json`, one entry
per fault with its path, its code and the numbers it compared.

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

```rust
const HANDLE: Text = Text::new().min(3).max(16).slug();

zod::accepts!(HANDLE, "ada-lovelace");
```

Change it to `"A"` and the build stops:

```
error[E0080]: evaluation panicked: too short
 --> src/main.rs:6:1
  |
6 | zod::accepts!(HANDLE, "A");
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^ evaluation of `_` failed inside this call
```

The same `HANDLE` goes in a `#[check(...)]`, where it reads the wire.

## One struct, every source

`Input` carries the data and says what the source is made of. That, not the
schema, decides whether text may become a number.

| Source | Shape | Text becomes a number |
|---|---|---|
| `Input::json(v)` | typed | no — a JSON body that sends `"36"` has a bug, and hiding it makes it permanent |
| `Input::form(body)` | flat | yes — a form has no types to lose |
| `Input::query("?page=2")` | flat | yes |
| `Input::env("APP_")` | flat | yes |
| `Value` you built | typed | no |

A field that must accept text from a typed source says so itself, with
`.loose()`.

Three form rules worth knowing, each of them a bug somebody has shipped: a
field with no value is nothing, not empty text; a name that repeats becomes a
list, which is what a group of checkboxes sends; an unticked checkbox sends
nothing at all, and a boolean reads that as false.

## The refusal

Every entrance carries one, so no boundary in the program invents its own.

| Refusal | Gives back |
|---|---|
| `Summary` (the default) | `String`, one line per fault |
| `Fields` | `Vec<(field, Vec<message>)>`, for a form that redraws itself |
| `Problem` | `Refused { status, content_type, body }`, RFC 9457 |
| `Raw` | the `Faults` themselves |
| any `Fn(&Faults) -> T` | whatever you build: a page, a template, a log line |

```rust
let page = |faults: &Faults| render("signup.html", faults.by_name());

let signup = schema.enter(input, &page)?;   // Err is the page
```

`#[entry(refuse = Type)]` attaches one to a struct; `Schema::enter` takes one
for a schema built by hand. Write `Wording` for another language, and
`say_all(&faults, &wording)` words every fault with it.

## Faults

Every fault, never only the first, each with the path that finds it and the
name a person reads.

```rust
let faults = Signup::parse(body).unwrap_err();

faults.to_string();
// 3 faults:
//   Handle: Pick 3 to 16 lowercase letters, digits or hyphens
//   Email address: an email address
//   people.0.age: above the maximum (130, found 200)

faults.by_name();        // grouped for a person
faults.by_path();        // grouped for a form
faults.at("age");        // one field
```

A `Flaw` carries `code`, `says`, `limit` and `found` as separate fields, so a
caller can write its own message in its own language without parsing a
string. `#[says("...")]` replaces three machine messages on one field with
one sentence a person can act on.

## The two questions

Libraries confuse these. They are different questions with different answers.

- **parse**: *is this unknown input a valid `T`?* It converts. It can fail on
  the type before it ever reaches a rule.
- **check**: *is this `T`, which I already hold, one the rules accept?* No
  conversion. The compiler settled the type already.

```rust
Signup::parse(Input::form(body))?;   // from the wire
held.check()                         // from your own code
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

## What is here

- `#[derive(Entry)]` — the struct is the schema. `#[check]`, `#[nested]`,
  `#[name]`, `#[label]`, `#[says]` on a field; `#[entry(strict, refuse = _)]`
  on the struct. A field with no attribute reads by its own type.
- `Text` — `min` `max` `len` `filled` `starts` `ends` `holds`, and the shapes
  `ascii` `alnum` `digits` `hex` `lower` `upper` `slug` `email` `uuid` `date`.
  Every one of them `const`.
- `Int`, and `Int::of::<u8>()` for every other integer width, which refuses a
  number that would not survive the cast. `Real`, finite by default, because
  `NaN` passing a range check is a bug every time.
- `object(...)` up to twelve fields for a schema with no struct behind it,
  `list()`, `optional()`, `or(default)`, `refine()`, `says()`, `label()`,
  `loose()`.
- `Input::form` `query` `env` `json`, and `Refusal` with four shipped kinds.

67 tests. No dependency outside the standard library except `syn` for the
derive (default, `default-features = false` drops it) and `serde_json` behind
the `json` feature.

## What is not here yet

- **No regular expression.** A regex engine cannot run inside the compiler,
  and a rule that works only at run time does not belong in the const layer.
  Use `refine` for now.
- **No unions and no enums.** `#[derive(Entry)]` refuses an enum, and says
  why.
- **No transforms.** A transform changes the type, which makes `check`
  ill-defined. It needs a design, not an afternoon.
- **No map with dynamic keys.**
- **`u128` above `i128::MAX`** does not fit the comparison and is refused
  rather than wrapped.

## Why it is called zod

Because that is the shape people recognise. It is not a port. See above.

## Licence

MIT.
