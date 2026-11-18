# JAHE module and package system (designed)

Compiler 0.7 currently accepts a single `module path;` declaration per source file, but does not parse `import`, load multiple modules, enforce source visibility, or resolve packages. Everything in this document is designed syntax and behavior.

## Module identity, import, and dot access

`module net.http;` declares the canonical source module identity. `import net.http as web;` imports that module's public namespace under local name `web`. `module` does not import dependencies; `import` does not change the current module identity. A path-derived identity is allowed when unambiguous, so every file need not repeat a declaration.

`.` is the sole source operator for namespace/submodule, field, method, associated item, and tuple positional access. `::` is not JAHE syntax. The lexer recognizes dot without interpreting it. The parser constructs a generic member/call/index chain; symbol and type resolution assigns meaning afterward.

Import bindings:

```jahe
import net.http as web;  // web.get(...): exact module alias
import image.png as png; // png.decode(...): exact module alias
import std.io;           // std.io.println(...): root namespace import
pub import network.http as http; // public re-export named http
```

An aliased import binds the alias to the complete module path and does not inject the original root. An unaliased dotted import binds the path's first component as a root namespace (`import net.http;` makes `net.http.get()` available). Multiple unaliased imports with the same root are valid if they belong to the same package graph. A local or item declaration that collides with an import binding at the same scope is an error; an inner lexical binding may shadow an imported root for its block, and references in that block resolve to the inner value. Alias-to-target duplicates are idempotent; reusing the alias for a different target is an error. Distinct aliases for the same module are allowed.

`pub import path [as alias];` re-exports using exactly the same binding rule as `import`: an alias re-exports the exact target under the alias, while no alias re-exports the first path component/root namespace. Thus `pub import network.http;` exposes `facade.network.http`, while `pub import network.http as http;` exposes `facade.http`. The path must resolve inside the current package/dependency graph. A public re-export exposes that module namespace, but clients can still access only its public items; it never makes private functions, types, fields, or child modules public. Every public path segment/re-export chain is checked when a downstream client resolves it. Re-export cycles are rejected together with ordinary import cycles.

## Deterministic resolution

Name lookup uses separate type and value/module namespaces. Type positions resolve type names only. In expression position the compiler searches innermost lexical values, module-level values/functions, then imported module roots/aliases; type-associated lookup is considered only when no value/module root resolves. A same-scope import/value conflict is diagnosed rather than silently shadowed. Nested lexical shadowing is permitted and intentionally hides the import in that block. To access a shadowed import, move the use outside the shadowing scope; no escape syntax is provided.

For a chain rooted at a module namespace, the resolver finds the longest valid existing module prefix and then resolves the remaining item path. Thus with `import std.io;`, `std.io.println` resolves module `std.io` first, then public item `println`. With `import net.http as web;`, `web.client.get` may resolve module `net.http.client` if it exists; otherwise it resolves exported member `client` of `net.http`, then continues by type/category. A missing prefix yields unknown-module or unknown-submodule diagnostics. Import aliases always resolve before attempting a type-associated path.

For a value root, each member uses the current expression type and declaration tables. `user.profile.email` checks a visible field at each step. `user.get_name()` resolves an applicable method on the receiver type, then validates arguments and result. `user.get_name` is field access only. It does not become a method value automatically. If a field and method have the same name, dot without parentheses selects the field; parentheses select the method. If parentheses are present but no method exists, report unknown method even if a callable field exists. A callable field can be invoked explicitly as `(user.callback)()` because the parser first constructs a grouped field expression, then calls that resulting value. This removes silent field-to-method rewriting. An inherent method wins over a trait method of the same name; if two in-scope trait methods both apply and no inherent method resolves the call, emit an ambiguous-method diagnostic. A qualified static trait call `Trait.method(receiver, args)` is the disambiguation form.

For type-associated items, `Point.origin()` resolves `Point` in the type namespace when no expression/module root shadows it. Qualified paths can remove ambiguity: `geometry.Point.origin()` starts at imported module `geometry`. A same-spelled local value shadows an unqualified type-associated root; use the qualified module path or a different local name. Tuple positions such as `position.0` are parsed as tuple-index access, never as a struct field identifier. The type checker rejects an out-of-range tuple index statically.

Required diagnostics include unknown module, unknown submodule, unknown module member, unknown field, unknown method, private member, ambiguous member, invalid member access for a type, invalid tuple index, duplicate/conflicting import, shadowed or unknown alias use, and re-export of private/unreachable items. Each reports source location and the resolved or expected category where available.

## Compiler stage boundaries

1. **Lexing:** creates identifier, dot, integer, and other tokens. It does not determine whether a dot means namespace, field, method, associated item, or tuple slot. Numeric tuple projection is recognized as dot followed by a decimal integer token; float literals remain a single numeric token.
2. **Parsing/AST:** builds generic `Member(base, name-or-index)`, `Call(callee,args)`, and `Index(base,index)` expressions. `web.get()` and `user.get_name()` have the same syntax tree shape; no symbol table is needed to parse either.
3. **Name resolution:** collects module identities, imports/re-exports, lexical symbols, and item declarations. It binds roots, checks visibility/collisions, and annotates resolved symbol identities.
4. **Type checking/semantic analysis:** resolves member category using symbol data and expression types; checks field/method/associated signatures, tuple bounds, mutability, visibility, and calls. Semantic analysis must finish successfully before lowering.
5. **IR lowering:** lowers the resolved operation to backend-neutral IR. Namespace access becomes a symbol reference; a field becomes an aggregate operation; a tuple projection becomes a statically checked projection; a method becomes a normal resolved function call with receiver convention.
6. **Backend:** consumes verified, resolved IR. It must not infer source member meaning or guess whether to generate a call or load.

## Module graph, files, and cycles

The package root is selected by `jahe.toml`; by default source root is `src/`, `src/main.jahe` is the executable entry source, and `src/net/http.jahe` maps to module `net.http`. A module declaration, when present, must match its canonical path. A `src/net/http/` directory may contain a `main.jahe` for module `net.http` and child files such as `client.jahe` for `net.http.client`; conflicting file and directory mappings are errors. Tests under `tests/` are separate test roots.

All import cycles are errors, including cycles formed through `pub import`. The loader detects strongly connected components before semantic body checking and emits a cycle chain with source import edges. This deliberately favors simple deterministic initialization and interface resolution over mutually recursive modules. Functions within one module may still mutually recurse because that module's signatures are collected before checking its bodies. Cross-module mutual recursion is deferred; break the cycle by moving a shared interface/type to a third module.

## Visibility

Items are private by default and visible only within their defining module. A module becomes externally nameable through a `pub import` in its parent/owning module; the package root's public imports form its exported module surface. Each member and each nested namespace crossing is checked independently. Struct fields are private unless individually `pub`. Enum variants inherit enum visibility and cannot independently expose an otherwise private enum; payload fields inherit variant visibility. Trait methods are visible through the public trait; implementation members cannot widen access beyond the trait/type boundary. Public signatures may not mention private types or private fields in a way that makes the interface unnameable; this is a compile-time error.

## `jahe.toml` and `jahe.lock`

`jahe.toml` is the human-edited package manifest. `jahe.lock` is generated, deterministic dependency resolution state and should be checked into applications and libraries that need reproducible builds. A representative manifest:

```toml
[package]
name = "image-tool"
version = "1.2.0"
edition = "0.8"
source = "src"
entry = "src/main.jahe" # optional for library packages

[dependencies]
local_codec = { path = "../codec", version = "^1.0" }
git_codec = { git = "https://example.org/codec.git", rev = "0123456789abcdef0123456789abcdef01234567", version = "^2.1" }
json = { registry = "default", version = "~3.4" }
```

Package name is a stable lowercase identifier; package version is SemVer. Edition is the language compatibility edition, independent of package version; compiler 0.8 targets edition `0.8` initially, with an explicit compatibility check. `source` and `entry` are relative paths within the package. When `entry` is omitted, the package is a library rooted at `src/lib.jahe`; otherwise it is an executable package rooted at the declared entry. Local paths are relative to the manifest containing the dependency; the lock records normalized paths relative to the declaring manifest (never machine-specific absolute paths), and content changes invalidate the tree hash. Path dependencies must satisfy their declared version requirement.

Version requirements use SemVer ranges (`^`, `~`, exact, and comparator sets). A resolver selects one package name and source identity across the complete graph; constraints must intersect or resolution fails with a conflict path. It does not silently install multiple incompatible copies under one name. Prereleases are excluded unless explicitly requested. Git dependencies require a full immutable commit `rev`; floating branches/tags are not accepted in locked builds. Registry dependencies use a named registry (default registry may be omitted) and a SemVer requirement.

`jahe.lock` records lock format version, root manifest hash, language edition, compiler version, bundled standard-library version, runtime ABI version, target triple, normalized build configuration, each package name/version/source, Git commit where applicable, content SHA-256, and sorted dependency edges. Local path content changes invalidate its recorded tree hash until explicit update. Registry archives must match the SHA-256 published by the selected trusted registry index over HTTPS. This guarantees transport and content integrity relative to that registry; publisher identity depends on registry trust policy. Registry URLs/trust identities are recorded by the lockfile. Locked builds do not resolve new versions and fail if manifest, compiler/target configuration, runtime ABI or dependency hashes disagree. `jahe update` is the command that resolves ranges and rewrites the lockfile; ordinary `jahe build` requires a consistent lockfile when dependencies are present. No lockfile is required for a dependency-free single-file invocation. Reproducing executable bytes additionally requires the same compiler/toolchain, target linker and deterministic build settings; the lock alone fixes dependency inputs, not machine-specific toolchains.

The exact registry protocol, trust-root/key rotation, optional dependencies/features, workspace inheritance, target-specific dependency expressions, and multiple versions of the same package are deferred. A package name has one selected source identity in a graph; declaring it from two sources is a conflict. These are package-manager extensions, not core syntax ambiguities. See [the finalization report](specification-finalization-report.md) for rationale and implementation prerequisites.
