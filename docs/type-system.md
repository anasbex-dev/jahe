# JAHE type system (design)

This is a proposed static type system, not a claim about the compiler's current semantic analyzer. Current source types are `i32`, `i64`, `f32`, `f64`, `bool`, `str`, and `void`; see [language-spec.md](language-spec.md). The compiler currently rejects the other types described here.

## Type categories

Primitive values are `bool`, signed `i8/i16/i32/i64/i128/isize`, unsigned `u8/u16/u32/u64/u128/usize`, `f32/f64`, `char`, `void`, and `never`. Integers use two's-complement signed representation or modulo-width unsigned bit representation. Fixed widths are target-independent; `isize` and `usize` equal pointer width. Alignment is target ABI-defined, at most the type size subject to ABI rules. `char` is one Unicode scalar value. `void` is the absent function result and cannot be stored; `never` has no values and is produced by operations that cannot return.

Arrays `[T; N]` are fixed-size inline sequences. Slices `[T]` are unsized views and can only appear behind a reference/pointer or in a function parameter after desugaring to `&[T]`. Tuples are product values. Structs are nominal records. Enums are nominal tagged unions. References and pointers are described in [memory-model.md](memory-model.md). Function values have an explicit function type. Generic parameters are statically substituted; no runtime type erasure is implied.

`String`, `Box<T>`, `Vec<T>`, `Option<T>`, `Result<T,E>`, maps and iterators are library types, not primitive compiler types. `String` owns UTF-8 storage; `str` is a borrowed UTF-8 slice. `Option<T>` is `None | Some(T)`; `Result<T,E>` is `Ok(T) | Err(E)` (including `Result<void,E>`). Their concrete layouts are library/compiler ABI details until representation guarantees are finalized.

## Inference and literals

Function parameter and public item types are explicit. Local binding types can be omitted when an initializer has a single known type. A literal with an expected integer type is checked against it. Otherwise integer literals infer the smallest fitting `i32`, `i64`, then `i128`; larger magnitudes require an explicit unsigned annotation and must fit it. A negative sign is a unary operator, so the magnitude of the minimum signed value needs a special boundary-aware literal rule. Uncontextualized float literals are `f64`. `true` and `false` are `bool`; character literals are `char`; string literals are immutable static `str` views. An initializer does not establish a mutable variable's type differently from an immutable variable.

Generic arguments infer from arguments and expected result where unique. Ambiguous inference is a diagnostic; public generic declarations remain explicit. No implicit integer/float conversion, signed/unsigned conversion, or pointer coercion is allowed. `never` may coerce to any expected type because it produces no value. Shared references may reborrow immutably; mutable-to-shared reborrow is allowed for the borrow duration.

## Numeric behavior

The design baseline checks integer add, subtract, multiply, negation, and shifts for overflow in all build modes. Division and remainder by zero fail; signed minimum divided by -1 overflows. Bitwise operations act on integer bit patterns. A later explicit wrapping API/operator may provide modular arithmetic. Float operations follow IEEE 754 binary32/binary64; NaNs compare unequal including to themselves, and ordered comparisons involving NaN are false. Floating arithmetic does not panic for infinities/NaNs.

`value as T` is the explicit conversion spelling. Conversions between integers are range-checked; float-to-int requires a finite in-range value and follows truncation toward zero only after range checks. Integer-to-float may lose precision and should be explicit. Exact syntax for recoverable conversion failures is open: candidates are a `Result<T, ConversionError>` conversion function or checked-cast intrinsic. No conversion silently wraps or saturates.

## Equality and ordering

Primitive scalar equality is value equality. String equality compares UTF-8 content, not addresses. Composite equality is not synthesized automatically; types opt into a future equality trait. Ordering exists only where a type defines it. Comparisons yield `bool`; comparisons cannot chain. Pointer equality is address equality and does not establish validity. Float comparisons use IEEE semantics.

## Layout and recursion

Arrays are contiguous, with stride equal to element size rounded to its alignment. Struct field offsets are laid out in declaration order with target-required padding; field reordering is not allowed by default. No stable binary layout is promised until an explicit ABI representation is specified. Enums carry a discriminant plus enough aligned storage for the largest payload; exact tag size is unspecified without a representation annotation. Packed layout is unsafe and deferred. Zero-sized values may be supported but do not create distinct address identity.

By-value recursive types are rejected because they have infinite size. `struct Node { next: Option<Box<Node>> }` is finite because `Box` has fixed pointer-sized representation. Generic recursive types follow the same rule after substitution. Layout computation must detect cycles and report a type diagnostic rather than recurse indefinitely.

## Generics, traits, and callable types

Type parameters are introduced by `<T>` and constraints by `T: Trait + Other`. Initial implementation uses monomorphization. Generic constraints are checked at instantiation; trait coherence forbids overlapping implementations. Associated types, specialization, const generics, and dynamic dispatch are deferred. `dyn Trait` is an explicit dynamic-dispatch type requiring object-safety rules. Function types spell `func(A, B) -> R`; a function item may coerce to a compatible non-null function pointer. Closures require a separate environment/capture type and are deferred.

## Diagnostics and implementation boundary

Type errors should name the expected and actual types and point to the expression/declaration span. Invalid inference, conversion, layout, recursive size, and trait resolution each need stable diagnostic categories. The current semantic types are a closed seven-type subset and no aggregate or generic layout exists. This design cannot be used to infer that structs, enums, pointers, ownership, or the full integer family compile today.
