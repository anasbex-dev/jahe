# Backend contract (0.7.0)

JAHE keeps lexing, parsing, semantic analysis, typed IR, verification, and backend implementation separate. Backends consume `ir::VerifiedModule`, which is created by the independent verifier. The interpreter remains available through `run`; the Cranelift backend is invoked by `build` and emits a native object before linking.

## Cranelift object backend

`backend::native::NativeBackend` uses the mutually pinned Cranelift 0.137.2 crates. It detects the host ISA and currently accepts only Windows x86_64. It declares all JAHE functions before defining bodies so direct and recursive calls can be resolved. Each JAHE block maps to a Cranelift block. JAHE phi nodes become block parameters, and each branch passes the incoming SSA value associated with its predecessor. `FunctionBuilder` seals and verifies the translated control flow before object emission.

The object is COFF and uses the host Windows calling convention. `jahe build source.jahe` writes a sibling `.obj` and `.exe`. The build invokes `VsDevCmd.bat`, builds the sibling `jahe-runtime` crate offline, then uses `rustc` as the MSVC linker driver with the generated object and static runtime archive. The Rust driver supplies the matching CRT and Windows system libraries. The runtime exports versioned `jahe_rt_v1_*` C ABI symbols for typed output, memory, and panic termination. No LLVM SDK or LLVM runtime DLL is involved. The ABI contract is described in [runtime-abi.md](runtime-abi.md).

The native executable entry must be `fn main() -> i32`. A generated launcher calls the JAHE entry and passes its integer return through the Windows process exit status. The interpreter's existing `main() -> void` convention remains unchanged. `jahe runtime-info` reports runtime ABI and native host capabilities.

## Native IR support and diagnostics

The backend supports integer and floating constants, string constants, integer and floating arithmetic, signed integer comparisons, boolean comparisons, floating comparisons, internal function calls, `println`, returns, unconditional and conditional branches, unreachable traps, and phi values. Checked signed integer add/subtract/multiply and division guard against overflow; integer division also traps on zero. Floating arithmetic follows IEEE behavior. String values are represented as immutable NUL-terminated UTF-8 pointers.

String comparisons are rejected because pointer comparison would not preserve JAHE's lexical string semantics. External functions other than `println` are rejected because the project has no native host binding registry or source ABI declarations. Unsupported instructions return source-spanned native diagnostics. There is no implicit conversion or fallback implementation.

## Interpreter and host functions

The interpreter is unchanged and continues to validate host signatures against IR external declarations, dispatch registered host implementations, capture output, and report structured runtime errors. Host implementations remain interpreter-specific. Native `println` is the only native external binding; arbitrary host functions need an explicit ABI/runtime design before they can be called from generated code.

## Validation

`cargo fmt --check`, `cargo check`, and `cargo test` validate formatting, compilation, and existing behavior. The native integration test builds a source program containing arithmetic, comparisons, a function call, loop-carried SSA values, branches, and output; it checks the COFF object and executable, launches the executable independently, and compares stdout and exit status. This test requires Windows x86_64 and installed Visual Studio C++ Build Tools.
