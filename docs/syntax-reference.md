# JAHE syntax reference (designed syntax)

**Design status:** examples describe the proposed JAHE language and are generally not executable in compiler 0.7. The [implementation snapshot](language-spec.md) lists currently accepted forms. Normative decisions are in [language-specification.md](language-specification.md); grammar is in [grammar.ebnf](grammar.ebnf).

## File and modules

```jahe
module app;
import std.io as io;
import net.http as web;
```

`module` names this compilation unit. `import` makes another module's public namespace available in this file. A project path may supply identity when there is no declaration.

## Functions and bindings

```jahe
pub func add(left: i32, right: i32) -> i32 {
    return left + right;
}

func report() {
    let label: str = "ready";
    var count: i32 = 0;
    count = count + 1;
    io::println(label);
}

const PAGE_SIZE: usize = 4096;
```

`func` declares a function, `let` an immutable local, `var` a mutable local, and `const` a compile-time constant. Omitted function result means `void`. Parameters have explicit types. Local annotations may be inferred only from a clear initializer. Constants need compile-time expressions. No function overloading is planned.

## Primitive and composite types

```jahe
let signed: i64 = -12;
let bytes: [u8; 4] = [0, 1, 2, 255];
let view: &[u8] = &bytes;
let pair: (i32, bool) = (4, true);

struct Point { x: f64, y: f64 }
enum Event { Quit, Key(u32), Resize { width: u32, height: u32 } }
```

Scalar families: `bool`, signed `i8/i16/i32/i64/i128/isize`, unsigned `u8/u16/u32/u64/u128/usize`, `f32/f64`, `char`, `void`, and `never`. Library types such as `String`, `Box<T>`, `Vec<T>`, `Option<T>`, and `Result<T,E>` are not compiler primitives.

## Control flow and patterns

```jahe
if point.x > 0.0 {
    io::println("right");
} else {
    io::println("left or on axis");
}

while count < 10 { count += 1; }
for event in events { handle(event); }
loop { break; }

match event {
    Event::Quit => shutdown(),
    Event::Key(code) => handle_key(code),
    Event::Resize { width, height } => resize(width, height),
}
```

`match` must cover every enum case unless a wildcard arm handles the rest. `for`, `loop`, `break`, `continue`, and patterns are designed but deferred.

## Methods, traits, generics

```jahe
import std.io as io;
import std.core as core;

impl Point {
    func origin() -> Point { return Point { x: 0.0, y: 0.0 }; }
    func distance_squared_from(&self, other: &Point) -> f64 {
        let dx: f64 = self.x - other.x;
        let dy: f64 = self.y - other.y;
        return dx * dx + dy * dy;
    }
}

trait Display {
    func format(&self, output: &mut io::Formatter) -> core::Result<void, io::FormatError>;
}

func identity<T>(value: T) -> T { return value; }
```

`impl` groups methods/trait implementations. `trait` declares a capability; generic constraints use `T: Trait`. Inherent methods and static generic dispatch are intended. These features are not implemented.

## References and unsafe operations

```jahe
import std.core as core;

func read_first(bytes: &[u8]) -> core::Option<u8> {
    if bytes.len() == 0 { return core::Option::None; }
    return core::Option::Some(bytes[0]);
}

unsafe func write_raw(pointer: *mut u8, value: u8) {
    unsafe { *pointer = value; }
}
```

`&T` is a shared borrow, `&mut T` an exclusive borrow, `*const T`/`*mut T` raw pointers. Dereference, pointer arithmetic, and foreign calls require unsafe. The ownership and borrow rules remain a future compiler feature.

## Error handling

```jahe
import std.fs as fs;
import std.io as io;
import std.core as core;
import std.string as strings;

func load(path: str) -> core::Result<strings::String, fs::Error> {
    return fs::read_to_string(path);
}

func main() -> i32 {
    match load("config.txt") {
        core::Result::Ok(text) => { use_config(text); return 0; },
        core::Result::Err(error) => { io::println(error); return 1; }
    }
    return 0;
}
```

Recoverable errors use `Result`; optional values use `Option`; `panic` is unrecoverable. Postfix `?` is a proposed error-propagation convenience, not optional member access, and is not yet frozen or implemented.

## Foreign interface

```jahe
extern "C" func puts(text: *const u8) -> i32;

unsafe func announce(text: *const u8) {
    puts(text);
}
```

This sketches the proposed FFI boundary only. Exact C type mapping, layout attributes, linking declarations, and safety wrappers are deferred.

## Expressions and precedence

Calls, indexing, field access, and casts bind tightest; unary operators follow; then multiplicative, additive, shifts, bitwise `& ^ |`, comparisons, equality, `&&`, `||`, and assignment (lowest, right-associative, statement form). Comparisons cannot be chained. `as Type` is an explicit conversion. `&&` and `||` short circuit. See [the specification](language-specification.md) for the complete table and overflow/conversion rules.
