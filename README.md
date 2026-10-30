<p align="center">
  <img src="https://raw.githubusercontent.com/DHHwasRight/.github/main/profile/logo.png" width="96" alt="">
</p>

<h1 align="center">zod</h1>

<p align="center">One rule, written once, run by the compiler on your literals and by the program on everything that arrives at its door.</p>

> **Status:** The DHHwasRight org is named after RailsConf 2026. It is an
> attempt to implement DHH's claims (HTML over the wire, no build step, one
> database) in Rust, to see how far that gets. This project is mostly vibe
> coded: an LLM wrote most of it. It is a thought exercise. We use it for
> internal products where mission criticality is not needed.

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

## Two ways to say the same rule

```rust
#[check(Text::new().min(3).max(16).slug())]   // the rule as a value
#[text(min = 3, max = 16, slug)]              // the rule as an annotation
```

They build the same schema. The first is for a rule that is shared, named
or reused — a `const` two structs point at is one rule, and the compiler can
run it against a literal. The second is for a rule that belongs to one field
and is read once, which is most of them.

| | |
|---|---|
| `text` | `min` `max` `len` as numbers, `starts` `ends` `holds` as text, and one of `filled` `ascii` `alnum` `digits` `hex` `lower` `upper` `slug` `email` `uuid` `date` |
| `int` | `min` `max` `step` as numbers, and `positive` `negative` `unsigned` |
| `real` | `min` `max` as decimals, and `positive` `wild` |
| `list` | `min` `max` as numbers, and `filled` |

The type does the rest. An `Option<T>` field is optional and a `Vec<T>` field
is a list, both read off the type as written, so the annotation describes one
value and never repeats what the field already says:

```rust
#[text(min = 2)]
#[list(min = 1, max = 3)]
tags: Vec<String>,        // 1 to 3 tags, each at least 2 characters

#[int(min = 1, max = 99)]
quantity: u8,             // and a u8 keeps its own range whatever the rule said
```

## A callback the struct carries

Two kinds, because a struct needs both.

**A rule that needs every field at once**, for the checks no single field can
make:

```rust
#[derive(Entry)]
#[entry(rule = passwords_agree, says = "The two passwords are not the same")]
struct Password {
    #[text(min = 8)] password: String,
    #[text(min = 8)] again: String,
}

fn passwords_agree(it: &Password) -> bool { it.password == it.again }
```

It runs after the fields and only on a value that exists. "The two dates are
the wrong way round" means nothing while one of them is still not a date.

**A refusal**, which turns every fault into whatever the boundary owes:

```rust
zod::refusal! {
    pub SignupPage(faults) -> String {
        faults.by_name().into_iter()
            .map(|(name, says)| format!("<li>{name}: {}</li>", says.join("; ")))
            .collect::<Vec<_>>().join("")
    }
}

#[derive(Entry)]
#[entry(refuse = SignupPage)]
struct Signup { /* .. */ }

let page = Signup::enter(input)?;   // Err is the page
```

## A value on the way out

`Schema::read` turns input into a struct; `Give` turns a struct back into
data, and `zod::write` renders it as JSON. The derive writes both, so one
struct is the whole contract of an endpoint — what it takes, what it gives,
and what a document says about either.

```rust
let made = Created { id: "0191a3f0-…".into(), seats: 3 };
zod::write(&made.give());   // {"id":"0191a3f0-…","seats":3}
```

Fields come out in the order the struct declares them, because a body a
person diffs should not reorder itself between two runs. No serde and no
reflection: the derive read the fields, and a field's type says how to
render itself.

## A schema describes itself

```rust
let node = Signup::describe();

node.name();            // Some("Signup") — the struct's own name
node.object().unwrap(); // fields, each with label, says, required, node
node.named_parts();     // every named struct in the tree, once, outermost first
```

`required` is derived from the schema rather than declared, so the two cannot
disagree: a field is required unless its schema accepts nothing. A `u8` field
describes the range it actually accepts, not the one the rule wrote.

This is what an OpenAPI document, a form widget and a command-line flag are
all readings of. `Schema::node()` gives the same tree for a schema built by
hand.

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

- `#[derive(Entry)]` — the struct is the schema. `#[check]` or
  `#[text]`/`#[int]`/`#[real]`/`#[list]`, plus `#[nested]`, `#[name]`,
  `#[label]`, `#[says]` on a field; `#[entry(strict, refuse = _, rule = _)]`
  on the struct. A field with no attribute reads by its own type.
- `Schema::node()` and `Entry::describe()` — the rules as data, for a
  document generator or a command-line builder. `zod::refusal!` names a
  callback in three lines.
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

97 tests. No dependency outside the standard library except `syn` for the
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

## Part of DHHwasRight

- [brocanth](https://github.com/DHHwasRight/brocanth): Rust framework for HTML-first apps and API services. Postgres-first, day-2 ready. Depends on zod for input, middleware, client and OpenAPI.
- [rsx](https://github.com/DHHwasRight/rsx): Compile-time HTML templates for Rust.
- [jt](https://github.com/DHHwasRight/jt): Compile-time JSON templates for Rust.
- [strata](https://github.com/DHHwasRight/strata): Layered async caching that reads like memoization.
- [depoth](https://github.com/DHHwasRight/depoth): A static content framework in Rust. Templates compile; content does not.
- [prerender](https://github.com/DHHwasRight/prerender): A static site generator for prerendered partial navigation.
- [htmx](https://github.com/DHHwasRight/htmx): dp-swap: partial page updates for prerendered sites. An htmx rewrite in TypeScript.

## License

MIT, as `Cargo.toml` states. The repo has no LICENSE file yet.
