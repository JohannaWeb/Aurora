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

- `interface Name [: Parent] { ... };`, `interface mixin Name { ... };` and
  `Name includes Mixin;`. No partials or namespaces. A mixin gets one
  `<Mixin>Members` trait; each including interface gets its own callbacks,
  brand-checked against that interface.
- `dictionary Name [: Parent] { ... };` with `required` members and literal
  defaults. Each becomes a Rust struct (a member that is neither required
  nor defaulted is an `Option`) and a conversion function that follows
  WebIDL: `undefined`/`null` give the defaults, other non-objects throw
  `TypeError`, and members are read once each, inherited ones first, each
  dictionary's sorted by name. Dictionaries are only allowed as non-null
  operation arguments; an optional one must default to `{}`. Nested
  dictionaries are rejected.
- `const` integer or boolean members, installed on the interface object and
  its prototype (read-only, enumerable, non-configurable).
- `[readonly] attribute T name;` as accessor properties on the prototype.
- `T name(args);` regular operations. Overloads and `any` are rejected;
  `optional` arguments may have literal defaults, and a final `T... name`
  argument collects the remaining arguments into a `Vec`.
- Types: `DOMString`/`USVString`, `boolean`, all integer types, `float`,
  `double` and their `unrestricted` forms, interface types, `undefined` as
  a return type, `T?`, and the union `(Node or DOMString)` (a
  `NodeOrString`: nodes stay nodes, anything else converts to a string). `NodeList` and `HTMLCollection` are allowed only
  as non-null read-only attribute and operation result types, and return
  a fresh array, as the hand-written members did, until those interfaces
  exist.
- `stringifier attribute DOMString name;` (string types only): the attribute
  plus a `toString()` that returns it.
- Extended attributes: `[CEReactions]` and `[NewObject]` (no generated
  effect) on members, and `[LegacyNullToEmptyString]` on non-nullable
  string types.

For each interface with members, `codegen/generate.rs` emits a
`<Interface>Members` trait that `Dom` in `src/js_v8/dom_impl.rs` implements,
so a declared but unimplemented member is a compile error. Generated
callbacks brand-check `this` against the generated inheritance chain, check
arity, convert arguments per WebIDL (ConvertToInt wrapping, non-finite
rejection for restricted floats, interface brand checks), wrap
`[CEReactions]` members in a reactions scope, and map errors to `TypeError`
or a native `DOMException`.

Members currently on generated bindings: `Node`'s constants, its tree
operations (`appendChild`, `insertBefore`, `replaceChild`, `removeChild`,
`cloneNode`, `normalize`, `hasChildNodes`, `isSameNode`, `isEqualNode`,
`compareDocumentPosition`, `contains`, `getRootNode`) and its attributes
(`nodeType`, `nodeName`, `nodeValue`, `isConnected`,
`ownerDocument`, `parentNode`, `parentElement`, `childNodes`, `firstChild`,
`lastChild`, `previousSibling`, `nextSibling`, `textContent`); `Element`'s
`namespaceURI`, `prefix`, `localName`, `tagName` (an element is SVG when
`interface_for` maps its tag to SVGElement, otherwise HTML; SVG names keep
their case), `id`, `className`, selector queries (`querySelector`/`All` on
ParentNode, `matches`, `webkitMatchesSelector`, `closest`; invalid selectors
throw SyntaxError, though the parser still accepts some unknown
pseudo-classes; matching is scoped to the node's shadow root),
`getElementsByTagName`/`ClassName` on Element and Document (descendants
only, as arrays), the ChildNode mixin (`before`, `after`, `replaceWith`,
`remove` on Element, CharacterData and DocumentType; no @@unscopables yet),
and attribute methods (`get`/`set`/`remove`/`has`/`toggle`
`Attribute`, their `NS` forms, `hasAttributes`; names match exactly and the
`NS` forms ignore the namespace until Aurora stores one); the
`HTMLHyperlinkElementUtils` mixin on `<a>` and `<area>` (`href`, `origin`,
`protocol`, `username`, `password`, `host`, `hostname`, `port`, `pathname`,
`search`, `hash`, via `url::quirks`, resolved against `location.href`;
`<base href>` is not applied yet);
the `ParentNode`, `NonDocumentTypeChildNode` and `InnerHTML` mixins; and all
of `CharacterData` (`data`, `length`,
`substringData`, `appendData`, `insertData`, `deleteData`, `replaceData`).
CharacterData offsets count UTF-16 code units; strings cross into Rust as
UTF-8, so lone surrogates are stored as U+FFFD. Everything else is still on the hand-written wrapper
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

## ShadyDOM descriptor mode

YouTube's ShadyDOM checks whether `Node.prototype.firstChild` is a
configurable getter. If it is, ShadyDOM copies native accessors from the
prototypes (`Node`: `parentNode firstChild lastChild previousSibling
nextSibling childNodes parentElement textContent`; `Element`:
`firstElementChild lastElementChild children childElementCount
previousElementSibling nextElementSibling innerHTML className`) instead of
using TreeWalker fallbacks. Moving only some of them to prototypes leaves the
rest `undefined` in ShadyDOM (`tp-yt-iron-icon` then throws reading
`childNodes`), so all sixteen moved together.

ShadyDOM also copies `appendChild insertBefore removeChild replaceChild
cloneNode contains` from `Node.prototype` as its `__shady_native_*` methods.
`v8_post.js` still wraps `appendChild`, `insertBefore` and `removeChild` on
`Node.prototype` with the adopted-shadow-root redirect (keeping the native
`length`). Page code must not take the redirect, so `__aurora_decorate_element__`
gives each element wrapper own aliases of the three unwrapped generated
methods, matching the old split between ShadyDOM's captured natives and the
wrappers' own methods. Measured 2026-11-21 on the replay captures: without
the redirect, search gains 100 elements (2127 → 2227) with duplicate and
misplacement counts still at 0. Removing the redirect and the aliases
belongs with the other compose heuristics (prs.md P2.4).
