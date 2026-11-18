# JAHE type system (designed)

The type system below is designed, not implemented by compiler 0.7. Its source types are currently only `i32`, `i64`, `f32`, `f64`, `bool`, `str`, and `void`, as documented in [language-spec.md](language-spec.md).

## Primitive types and target representation

Primitives are `bool`; signed `i8/i16/i32/i64/i128/isize`; unsigned `u8/u16/u32/u64/u128/usize`; `f32/f64`; `char`; `void`; and `never`. Fixed-width integers use exactly the declared width, two's-complement signed representation or unsigned modulo bit patterns. `isize`/`usize` match pointer width. `bool` has logical false/true values; its in-memory representation is one byte, while C ABI functions must use `u8` rather than assume C `_Bool` compatibility. `char` is a Unicode scalar, stored as 32-bit value; invalid scalar encodings cannot form `char`. `void` is no result and cannot be stored. `never` has no values and coerces in a value context because it does not return.

Alignment is target ABI-defined. Cross-compilation uses the selected target layout and never the host's layout. Compiler 0.7 only implements Windows x86_64/MSVC. Default struct layout preserves declaration field order with target-required padding; enum layout uses a discriminant plus aligned payload. Exact default enum tag width, niche optimization, padding bytes, and cross-version in-memory stability are compiler-private and not promised. A future `repr(C)` annotation is deferred; until it is defined, aggregates cannot cross FFI. No packed layout is supported in the initial language.

## Literals, inference, and conversion

Function parameters, return types, public items and external signatures require explicit types. Local binding inference is allowed only from a unique initializer; it does not flow backward from later use. Empty arrays, `None`, and generic values without a unique expected type need an annotation. Type names and value/module names use separate namespaces. In expression lookup, lexical value bindings shadow imports in nested scopes; same-scope item/import collisions are rejected. Type-associated lookup is tried only when no value/module root resolves; qualify it through its module when a value shadows it.

Integer literals use expected integer context when present, otherwise the smallest fitting `i32`, `i64`, then `i128`; values outside signed `i128` require an explicit fitting unsigned type. A negative sign is a unary operation, with a special parse rule so each signed minimum can be represented. Float literals default to `f64`; `f32` requires context or annotation. Integer add/subtract/multiply/negate and left shift are checked identically in debug/release and panic on overflow. Wrapping is explicit through `wrapping_add`, `wrapping_sub`, `wrapping_mul`, and `wrapping_shl`; shift counts outside bit width panic rather than being masked. Signed right shift sign-extends; unsigned right shift zero-fills. Division and remainder by zero panic; signed min divided or remaindered by -1 also panics. Bitwise operations preserve the fixed-width bit pattern.

There are no implicit numeric conversions. `value as T` is explicit, checked, and panics if the value is out of range or invalid. Float-to-int truncates toward zero after finite/range check. Integer-to-float follows IEEE rounding and can lose precision. `std.core.try_as(value)` with an expected `Result<T, CastError>` type returns `Result<T, CastError>` for recoverable failure. Invalid casts report a compile-time diagnostic when statically known; dynamic failure uses a panic message naming source/target types. No reinterpret cast is provided in safe syntax.

## Aggregates, strings, and equality

`[T; N]` is a fixed inline array; `&[T]` is a borrowed pointer-plus-length slice. Tuples are positional products. `(T)`/`(value)` is grouping and `(T,)`/`(value,)` is a singleton tuple; empty tuple/unit is not a type/value. Tuple projections use dot and a decimal integer: `pair.0`. Projection bounds are statically checked; using an out-of-range index is a type/member diagnostic. A tuple index token is numeric and cannot name a struct field. Structs are nominal records; fields default private. Enums are tagged sums. By-value recursive layouts are rejected; indirect recursion through `Box<T>`/references is permitted.

`String` is owned growable UTF-8; `str` is a borrowed UTF-8 byte slice; literals are static borrowed `str`. Length means bytes; character iteration decodes Unicode scalars. Embedded NUL is valid. Equality compares string contents without Unicode normalization. Primitive scalars support value equality. Aggregate equality/ordering is not synthesized; a future trait implementation opts in. Pointer equality compares addresses but proves no validity. Float comparisons use IEEE rules, including NaN unequal to itself.

## References and ownership types

`&'a T` is a shared reference and `&'a mut T` an exclusive mutable reference; the lifetime may be inferred for local temporaries. Raw pointers are `*const T` and `*mut T`. Safe references are non-null, aligned, initialized, valid, and cannot outlive the referent. The detailed ownership rules are in [memory-model.md](memory-model.md). References and raw pointers are not implemented.

## Generics, traits, and monomorphization

Generic functions/types use `Name<'a, T: Trait>` parameter syntax. Lifetime parameters precede type parameters. Bounds use `T: Trait + Other`. Inference must yield one unique instantiation; ambiguity is diagnosed. Compiler generates one concrete instance per used type/const-free type parameter set (monomorphization); no specialization or implicit dynamic erasure. Recursive instantiation is bounded by detecting repeated normalized types; unbounded type growth diagnoses recursion overflow. Const generics are deferred.

`trait` declares methods and default implementations. Initial traits have no associated types; encode alternatives with generic parameters or related traits. An impl is legal only if its trait or target type is defined in the current package; two impls that could match the same concrete type are rejected. This avoids global coherence ambiguity. Static dispatch is the default. `dyn Trait`, vtables and object safety are deferred; if introduced, object-safe traits cannot expose generic methods or `Self` in non-receiver positions.

## Functions, pointers, and closures

Function type syntax is `func(A, B) -> R`; omitted result is `void`. A function item may coerce to a non-null function pointer of matching signature. Nullable callback storage is `Option<func(A) -> R>`. C ABI callbacks use an explicit `extern "C" func(...) -> ...` type, only fixed-width scalars/raw pointers, and unsafe invocation. Function pointers own no environment and cannot capture values.

Closures are deferred. The selected future direction requires explicit capture lists (`|move x, &y, &mut z| ...`); capture defaults are forbidden. `move` captures transfer ownership; shared/mutable captures create borrows and cannot outlive the source. Escaping mutable captures require a safe heap environment and full borrow checking. Diagnostics include unknown captures, use after move, conflicting borrow, lifetime escape and incompatible call signature. Closure traits (`Fn`-like) and exact grammar remain deferred until ownership/generics are available.

## Generic library types

`Box<T>`, `Vec<T>`, `String`, `Option<T>`, `Result<T,E>`, `HashMap<K,V>`, `HashSet<T>`, and `Iterator<T>` are library types, not compiler primitives. `Option<T>` models absence; `Result<T,E>` models recoverable errors. Their exact layouts remain private except at explicitly defined ABI boundaries. None is available in current JAHE source.

## Diagnostics and prerequisites

The checker reports mismatched types, missing annotations for ambiguous inference, numeric literal overflow, invalid casts, invalid tuple projection, recursive by-value layout, unsatisfied generic bounds, incoherent trait impls, and ABI-incompatible signatures. The prerequisites are frontend type definitions, target data layout, aggregate IR, generic instantiation, trait resolution, and sound ownership analysis. No syntax described here should be treated as implemented.
