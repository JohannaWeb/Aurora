# WebIDL interfaces

These checked-in WebIDL files describe Aurora's initial interface inheritance
graph and the members that have moved onto generated bindings. They are
curated declarations based on the
[DOM](https://dom.spec.whatwg.org/),
[HTML](https://html.spec.whatwg.org/multipage/indices.html#element-interfaces)
and [SVG](https://svgwg.org/svg2-draft/types.html#InterfaceSVGElement) standards,
reviewed 2026-11-15, not complete or unmodified upstream IDL files.
`html-tags.txt` supplies the separate HTML tag-to-interface mapping.

`build.rs` parses the files and emits Rust into `OUT_DIR/web_interfaces.rs`.
Generated code builds V8 FunctionTemplates, inherits their prototypes, installs
interface objects and `Symbol.toStringTag`, and maps tags to classes. The
registry retains intrinsic prototypes, so replacing a global constructor cannot
change the class of newly created DOM wrappers. `HTMLDocument` is the legacy
alias of `Document`.

## Supported IDL (P2.2)

The parser accepts a deliberate subset and rejects everything else, so
unsupported IDL fails the build instead of producing wrong bindings:

- `interface Name [: Parent] { ... };`, no partials, mixins or namespaces.
- `const` integer or boolean members, installed on the interface object and
  its prototype (read-only, enumerable, non-configurable).
- `[readonly] attribute T name;` as accessor properties on the prototype.
- `T name(args);` regular operations. Overloads, variadics and `any` are
  rejected; `optional` arguments may have literal defaults.
- Types: `DOMString`/`USVString`, `boolean`, all integer types, `float`,
  `double` and their `unrestricted` forms, interface types, `undefined` as
  a return type, and `T?`.
- Extended attributes: `[CEReactions]` only.

For each interface with members, `codegen/generate.rs` emits a
`<Interface>Members` trait that `Dom` in `src/js_v8/dom_impl.rs` implements,
so a declared but unimplemented member is a compile error. Generated
callbacks brand-check `this` against the generated inheritance chain, check
arity, convert arguments per WebIDL (ConvertToInt wrapping, non-finite
rejection for restricted floats, interface brand checks), wrap
`[CEReactions]` members in a reactions scope, and map errors to `TypeError`
or a native `DOMException`.

Members currently on generated bindings: `Node`'s constants and
`Node.contains()`. Everything else is still on the hand-written wrapper
templates in `node_create.rs` and migrates in P2.3–P2.5. Missing parents,
duplicate names, unknown types and inheritance cycles fail the build.

Constructors other than `HTMLElement` retain their placeholder behavior; the
native `HTMLElement` entry point delegates to the existing custom element
construction hook. Namespace storage and SVG subclass selection are deferred.

## Validation, 2026-11-16

`cargo test --lib` passes 291 tests serially (one ignored), as do six visual
regressions and `cargo clippy --lib --bins -- -D warnings`.

`make wpt` passes 1583 subtests, up from 1423 in `tests/wpt/expectations.json`.
Gains come from wrappers now being real instances of their interfaces:
customized built-ins (`builtin-coverage.html`, +47), `Node-cloneNode.html`
(+71), node realm/adoption tests, and `attachShadow` returning an
`instanceof ShadowRoot`. A few subtest names embed the constructor name
(`PatchedHTMLElement` → `HTMLDivElement`, `[object Object]` →
`[object HTMLHeadingElement]`), so they show as removed and re-added.
`insertion-removing-steps/change-event.window.html` goes from OK to TIMEOUT
with 0 passes either way. `document.contains` no longer throws (it returns
`undefined`), so testdriver now waits for manual key input, which a headless
run never provides. The wrong return value moves with `Node` in P2.3.

The release binary present at the start of P2.1 was compared with the new
release build using the same four replay captures. Every non-timing metric
is identical; existing JS errors and replay misses remain:

| Capture | Elements | Items | Videos | Duplicates | Misplaced | JS errors | Replay misses |
|---|---:|---:|---:|---:|---:|---:|---:|
| Channel | 273 | 0 | 0 | 0 | 0 | 0 | 0 |
| Home | 1291 | 0 | 0 | 0 | 0 | 2 | 0 |
| Search | 2125 | 13 | 8 | 0 | 0 | 2 | 11 |
| Watch | 1538 | 0 | 0 | 0 | 0 | 0 | 1 |
