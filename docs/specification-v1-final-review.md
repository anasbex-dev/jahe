# JAHE specification final technical review

## Outcome: READY FOR USER APPROVAL

This report recommends the design for explicit user approval. It does not declare the specification approved or frozen. Only the user may authorize calling the language contract JAHE Language Specification v1.0. All compiler/runtime features described as designed remain unimplemented unless separately identified as compiler 0.7 behavior.

## Executive summary

The soundness review found correctable grammar gaps and underspecified semantics, but no core design blocker. The grammar now admits explicit dereference places and `self`, while constraining assignments to places. The design now fixes `mut` as reserved, defines reborrow and control-flow loan rules, sets temporary and raw-pointer boundaries, adopts one explicit discard form, distinguishes registry content integrity from identity/authenticity, and gives executable packages one private `i32 main` selected by manifest `entry`.

## Issues found and corrections applied

| Issue | Correction |
|---|---|
| `*pointer = value` appeared in designed syntax but assignment grammar only allowed identifier-rooted places. | Added `place`, `place-root`, `dereference-place`, and projections. Only locals/`self`, grouped places, and explicit dereferences can start an assignment target; arbitrary unary expressions are excluded. Semantic checks require writable storage; raw writes require `*mut T` and `unsafe`. |
| `self.value` examples were not representable and receiver scope/ownership were unclear. | Added `self` as a primary and place root. It is valid only with a method receiver; associated functions cannot use it. By-value `self` moves into an immutable receiver binding, `&self` shares, and `&mut self` exclusively borrows. |
| `mut` had no reserved/contextual status. | Chosen as a reserved keyword, restricted to mutable references, mutable raw pointers and `&mut self`; the language keyword list, type rules, grammar comments and syntax docs agree. |
| External declarations could grammatically use method receivers. | Added a separate external parameter grammar with named typed parameters only; unsupported ABI strings are semantic diagnostics (initial ABI is `C`). |
| Reborrow, field splitting, CFG loans, temporary lifetime, and raw-pointer alias constraints were missing. | Added conservative initial rules below and in [memory-model.md](memory-model.md); advanced provenance details remain deferred. |
| `Result`/`Option` could be ignored without a settled discard form. | Selected `let _ = expression;` as the sole explicit discard syntax; `_` is not a binding and remains a wildcard pattern. `Result`/`Option` are must-use. Ordinary unused non-void expression statements warn; `void` expression statements are valid. |
| Registry hash wording could imply publisher authentication. | Clarified HTTPS transport, hash-relative content integrity, lock behavior, identity/authenticity limits, source confusion risks and deferred trust mechanisms. |
| Entry point varied between public/private and had inconsistent package semantics. | `entry` explicitly makes a package executable. It designates one source with exactly one private, non-generic, parameterless `func main() -> i32`. Omission means library; any library `main` is ordinary. No argv or `void` entry is initially supported. |

## Final ownership and borrowing decisions

- Reborrowing `&mut T` to `&T` or `&mut T` temporarily suspends the parent. The parent becomes usable after the child’s last use on all reachable paths. The child cannot outlive or escape the parent. `&T` cannot be reborrowed mutably.
- Distinct declared struct fields and tuple positions are independently borrowable, including nested paths whose projections diverge at a known field. Whole-object access overlaps its projections. Dynamic indices may alias, so uncertain overlap is rejected. Partial-move restrictions for destructor types still apply.
- Loans are tracked by place and last use through branches, matches, loops, exits and reassignment. Joins merge conservatively; loops are analyzed to a fixed point. Reassignment that invalidates a live loan is rejected. Local references need no lifetime syntax; explicit names relate function input/output borrows where elision is ambiguous.
- Temporaries last through the full statement and are destroyed in reverse creation order. A temporary directly borrowed to initialize a local reference is extended to that local’s scope. Call-argument temporaries survive through the call; references to temporaries cannot be returned. Invalid escape/use reports source and use spans.
- Safe reference and `Box` dereference are safe under normal borrow checks. Raw pointers retain provenance and derived loans. Reference-to-raw conversion preserves mutability (`&T` to `*const T`, `&mut T` to `*mut T`); shared references cannot produce mutable pointers. Raw dereference/write and conversion back to safe references require `unsafe`; reconstitution must establish validity, lifetime and exclusivity. Unsafe code cannot relax guarantees relied on by safe references. Advanced provenance/tagging semantics are deferred.
- Assignment evaluates a place once before the RHS. Simple assignment drops the old initialized destination after RHS completion and then stores; moved fields are not dropped twice and may be reinitialized. Compound assignment evaluates the place once, reads its old value before the RHS, applies checked arithmetic and stores. Arithmetic panic aborts before write. Assignment remains statement-only. Destructors cannot panic; early exits drop exited scopes in reverse initialization order; process abort runs no destructors.

## Explicit discard decision

The selected form is `let _ = expression;`. It is the only explicit discard syntax and works for any expression value. `Result` and `Option` are must-use and otherwise must be consumed, matched, returned or propagated with exact-type `?`. `_` never creates a readable local; it is also a wildcard pattern. No `discard` keyword is added. Unused ordinary non-void results warn; void expression statements are allowed. This avoids a second, redundant statement form.

## Registry trust boundary

- HTTPS protects transport to the configured endpoint, subject to the endpoint and certificate trust chain.
- SHA-256 detects bytes that do not match the expected locked digest; it does not prove who authored/published the bytes.
- Registry identity is recorded as a source in the lock, but the initial design has no cryptographic registry identity or signed metadata requirement.
- Publisher identity and authenticity are not guaranteed. A malicious publisher, compromised registry, or attacker controlling first resolution can supply malicious content.
- Locked builds reproduce the selected graph relative to the lock’s recorded sources and hashes, and fail on mismatches; they do not authenticate a lockfile that has itself been maliciously changed or a first resolution.
- One package name may resolve from one source identity in a graph; conflicting sources fail, preventing silent substitution during an unchanged locked build. The policy does not prevent deliberate manifest/lock edits or source confusion at initial resolution.
- Registry signatures, trust roots, publisher verification, key rotation and recovery policy are deferred. A separate trust and key lifecycle design is required before making stronger supply-chain claims.

## Final entry-point contract

For a package with `entry`, the selected source defines exactly one private `func main() -> i32`, with zero parameters, no generic parameters, and no overload. It is a normal function recognized as the process entry only by the package build. Its result maps to the target platform process exit status. An executable package cannot define another `main` in another source module. A package without `entry` is a library and has no process-entry requirement; a private library function named `main` is ordinary. Compiler 0.7’s existing `fn main` interpreter/native conventions are implementation facts and remain separate from this designed syntax.

## Grammar and cross-document validation

The EBNF was reviewed against the documented designed forms: modifiers/declarations, C extern declarations and function types, generic/lifetime syntax, aggregate and trait declarations, imports/re-exports, visibility, references/pointers, unsafe blocks, assignments/places, labeled loops, statement matches/patterns, tuple/array/struct literals, calls/indexing/members/tuple projections, `?`, casts, precedence, escapes and comments. A syntax construct may be rejected by compiler 0.7 while still being correctly represented in the proposed grammar.

Cross-document consistency checks confirmed:

- `func`, immutable `let`, mutable `var`, `const`, and reserved `mut`/`self` policy are described consistently.
- `.` is the only JAHE namespace/member operator; `::` occurs only in explicit non-JAHE or Rust-implementation explanations.
- Field/method punctuation, tuple `.N`, singleton tuple/grouping, `pub import`, visibility and cycle behavior agree in prose and examples.
- `Result`/`Option` must-use and explicit discard agree across grammar, specification, errors, library design and examples.
- Executable `entry` and private `func main() -> i32` agree across specification, decisions and package documentation; compiler 0.7’s `fn` rules remain explicitly implementation-specific.
- HTTPS, SHA-256 and lockfile guarantees are limited to their actual security properties.
- Designed and deferred features are not labeled implemented.

Markdown relative links and referenced example paths were checked. The final EBNF was inspected against all examples under `docs/examples/`; no compiler parser was invoked because these files intentionally illustrate designed syntax and the task is documentation-only.

## Deferred features and known limitations

Deferred topics include advanced raw-pointer provenance models; closures and async; unions and `repr(C)` aggregates; associated types, specialization and dynamic trait objects; block/`if`/`match` expressions; static/global storage; inline assembly, volatile, atomics, threading and synchronization; conditional compilation; workspaces/features, registry trust/signing/key lifecycle, multiple simultaneous package versions, and non-Windows ABIs. The proposed borrow checker remains a design and must pass soundness testing before JAHE claims safe code. Lockfiles cannot authenticate first resolution without a future trust mechanism.

## Exact files changed

- `docs/grammar.ebnf`
- `docs/language-specification.md`
- `docs/language-design-decisions.md`
- `docs/type-system.md`
- `docs/memory-model.md`
- `docs/module-system.md`
- `docs/error-handling.md`
- `docs/syntax-reference.md`
- `docs/standard-library-design.md`
- `docs/examples/errors.jahe`
- `docs/specification-v1-final-review.md` (this report)
- `README.md` (links to this review)

## Validation commands and actual results

- `git status --short` before editing: clean; no existing user modifications were present.
- PowerShell relative-link check over `README.md` and `docs/**/*.md`: **passed**, no broken relative Markdown targets.
- `Get-ChildItem docs/examples -File` and review of each file: **completed**; example paths exist and syntax was checked against the designed grammar and semantic rules.
- `rg -n '::' README.md docs`: **reviewed**; findings are explicit statements that `::` is not JAHE syntax and Rust implementation notation (`ir::`, `jahe_runtime::`, ABI symbol names), not JAHE source examples.
- `rg` searches for `main`, `self`, `mut`, discard, pointers, assignments, and `Result`/`Option`: **reviewed**; the only `fn main` and `void main` contracts are explicitly marked compiler 0.7 implementation behavior.
- Compiler tests/builds: **not run**, because implementation validation is outside this documentation-only task and was explicitly disallowed absent a specific reason/authorization.
- Git diff/status scope review: **documentation-only**. No compiler, runtime, dependency, manifest, LICENSE, or LLVM probe files were changed. No commit or push was made.

## Recommendation

**READY FOR USER APPROVAL.** The design corrections and documentation checks are complete, with no known blocking decision. The next step is your explicit approval to freeze JAHE Language Specification v1.0. Compiler 0.8 implementation should begin only after that approval and should follow the reviewed contract.
