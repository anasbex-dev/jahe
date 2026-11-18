# JAHE module and package system (proposed)

The compiler currently accepts one `module path;` declaration per file but has no `import` parsing, package manifest, multi-file resolution, visibility, or separate compilation. This document is the designed future module model.

## Identity versus import

`module net.http;` declares the source unit's canonical namespace identity. `import net.http;` makes the public names in that namespace available in the current file. An alias changes the local namespace name: `import net.http as web;`. These declarations are not interchangeable. A module declaration does not make dependencies available, and an import does not rename the current module.

When a file's path determines its identity unambiguously, the module declaration is optional. If present, it must match the path; this catches moved/copied files. `src/main.jahe` is the package root. `src/net/http.jahe` maps to `net.http`. Root source may declare `module app;` if the package name is `app`; this does not have to be repeated in each file.

## Imports and name lookup

Imports are file-scoped and appear before item declarations. `import a.b;` introduces `b` as a module namespace, and `import a.b as c;` introduces `c`. Qualified item names use `b::PublicName` or `c::PublicName` (module paths in declarations use dots). A later explicit item import may be added, but wildcard imports are excluded from the initial design because they obscure name origins. Imports expose only `pub` items. A private item is visible only in its defining module and its child implementation context according to the eventual privacy boundary; the design baseline treats privacy as module-scoped.

Repeated identical imports are idempotent. Two imports with the same local binding but different targets are errors. A local item conflicts with an imported namespace name unless explicitly qualified. Imports never implicitly re-export. Explicit `pub use`/re-export syntax remains open and is not in the grammar baseline.

## File tree and manifest

Proposed project shape:

```text
sample/
  jahe.toml
  src/
    main.jahe
    net/http.jahe
    image/png.jahe
  tests/
```

`jahe.toml` identifies package name/version, root source, compiler edition, target configuration, and dependencies. Exact schema, lockfile, registry and source integrity format are open. A package without a manifest is still buildable as one file through current CLI; it does not gain imports automatically. No `jahe.mod` sidecar is selected because one package manifest is enough to define roots and dependencies.

## Graph construction and compilation

The compiler resolves imports from the package root, standard library root, and declared dependency roots in that order with explicit ambiguity errors. Module identity is canonical and each source is loaded once. The compiler first discovers module declarations and exported signatures for the complete dependency graph, then type-checks bodies, allowing mutually recursive references across modules. Import cycles are therefore permitted for declarations, but dynamic global initialization is forbidden so there is no runtime initialization cycle. Type-only dependency cycles are permitted only if layout computation can topologically resolve by-value types; recursive aggregates must use indirection.

Separate compilation should cache typed module interfaces keyed by compiler edition/version, target ABI, source content, and dependency interface hashes. Cache invalidation is content-based; cached artifacts are never trusted without matching hashes. The initial implementation can compile the graph as one unit while preserving the interface boundary. Incremental/module caching and stable object interfaces are later work.

## Visibility

Items are private by default. `pub` makes a function, type, module, or constant accessible to importing modules. Struct fields are private by default and independently marked `pub`. Nested modules are private unless exported; a public parent must export child modules to make their paths reachable. Re-export design is open. `private` is unnecessary as a keyword.

## Standard library and external packages

`std.*` resolves from the compiler-distributed standard library versioned with the runtime/compiler compatibility contract. It is not a special parser namespace. `net.http`, `image.png`, and other specialized packages are ordinary dependencies. External package selection, version resolution, vendoring, checksum trust, offline behavior, and lockfile format require a separate package-management design. Nothing in this document claims such modules or a registry exist.

## Diagnostics

Required errors include missing module/file, path/declaration mismatch, duplicate module identity, duplicate/conflicting import alias, private item access, cyclic by-value layout, ambiguous module resolution, dependency version conflict, and invalid re-export. Diagnostics should show both the importing declaration and conflicting/declared module locations where relevant.
