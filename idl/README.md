# Interface skeletons (P2.1)

These checked-in WebIDL files describe Aurora's initial interface inheritance
graph. They are curated, member-free declarations based on the
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

The parser deliberately accepts only empty `interface Name [: Parent] {};`
declarations and comments. Members, extended attributes, partials and mixins
are rejected until their semantics are implemented. Missing parents, duplicate
names and inheritance cycles fail the build. Source order need not match
inheritance order.

Existing Rust wrapper members and JS behavior remain until P2.2–P2.5 migrate
them. Constructors other than `HTMLElement` retain their previous placeholder
behavior; the native `HTMLElement` entry point delegates to the existing custom
element construction hook. Namespace storage and SVG subclass selection are
also deferred; this step preserves the existing SVG classification.

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
