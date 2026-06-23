# Aurora — Rust Best-Practices & Code-Quality Analysis — 2026-06-23

**Scope:** Aurora-owned Rust in `src/` (135 files, ~27.9k LOC). This is a
*maintainability and correctness-hygiene* review, not a security review (see
`SECURITY_CVE_ANALYSIS-2026-06-23.md` for that) and not a bug hunt. It focuses on
the three things you flagged — `unwrap` density, unnecessary `Rc` plumbing, and
clone churn — plus the adjacent idiom issues that cluster with them.

**Posture:** Aurora is pre-usable, single-user, static-rendering. None of this is
"broken." The goal here is to separate *cosmetic* smells (fine to leave) from the
few patterns that will actively cost you during the blitz-dom migration or that
can panic the engine on hostile input.

---

## Metrics at a glance

| Signal | Count | Read |
|--------|------:|------|
| `.unwrap()` | 261 (213 outside test files) | High — but most are one specific, defensible pattern (see PR-1) |
| `.expect(...)` | 63 | Fine — these carry messages |
| `panic!`/`unreachable!`/`todo!`/`unimplemented!` | 19 | Worth auditing the `todo!`/`unimplemented!` subset |
| `.clone()` | 283 | Mostly cheap `Rc` bumps; a handful are real (PR-3) |
| `Rc<…>` uses | 168 | `NodePtr = Rc<RefCell<Node>>` dominates |
| `Rc<RefCell<…>>` | 49 | The shared-mutable backbone; expected for a DOM |
| `RefCell` | 132 | Runtime borrow-panic surface (PR-4) |
| `#[allow(dead_code)]` | 70 | High — signals premature/abandoned API (PR-5) |
| `unsafe` | 124 | Covered in the security report; not re-litigated here |

---

## PR-1 — `unwrap()` density: triage, don't carpet-bomb

213 non-test `unwrap()`s sounds alarming, but they are **not uniform**. They fall
into three buckets with very different risk, and the fix is different for each.

### 1a. Infallible V8 setup calls — *low risk, leave or thinly wrap*

The large majority (e.g. **75 in `node_create.rs`, ~45 in `runtime.rs`**) are V8
API calls during isolate/template construction:

```rust
// node_create.rs:428 / runtime.rs:247,329,343,...
let obj = template.new_instance(scope).unwrap();
v8::String::new(scope, s).unwrap();
add_event_listener_fn.get_function(scope).unwrap();
let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();   // ×40+
```

These only return `None` on OOM or a genuine programmer error (wrong template
wiring). Recovering is neither possible nor meaningful. **Recommendation:** leave
them, but collapse the most repeated ones behind tiny helpers
(`fn v8_str(scope, s) -> Local<String>`, `fn node_data(args) -> &NodeData`) so the
261 raw `unwrap()`s shrink to a handful of audited helper bodies. This is purely a
readability/auditability win — one place to change if you ever want graceful OOM.

### 1b. **Panics inside V8 callbacks unwind across FFI — this is the real hazard**

V8 invokes your callbacks (`aurora_fetch_sync`, the `node_create` methods, etc.)
from C++. The `rusty_v8` bindings do **not** wrap callbacks in
`catch_unwind`, so a panic unwinds *into* the V8 C++ frame. Under
`panic = "unwind"` that is undefined behavior; under `panic = "abort"` it kills the
process. So any `unwrap()` in a callback that can fail on **script-controlled
input** is a remote DoS / soundness footgun, not a benign panic:

```rust
// node_create.rs:803,807,815  — values come straight from JS
let obj = val.to_object(scope).unwrap();              // None if JS passed a primitive
let id  = id_val.int32_value(scope).unwrap() as u32;  // None on a non-number
let blitz_id = blitz_id_val.int32_value(scope).unwrap() as usize;
```

**Recommendation:** for callbacks, treat "bad JS argument" as a normal path —
`let Some(obj) = val.to_object(scope) else { return; }` (or throw a JS TypeError),
never `unwrap`. Optionally install one `catch_unwind` boundary at the callback
trampoline so a stray panic becomes a JS exception instead of UB. This is the one
sub-item I'd prioritize over the others, because it's correctness, not style.

### 1c. `todo!` / `unimplemented!` — *audit the 19*

Grep the 19 explicit panics; any `todo!`/`unimplemented!` reachable from JS or
layout on a real page is a latent crash. Convert reachable ones to a logged no-op
or a typed error.

---

## PR-2 — Unnecessary `&Rc<T>` parameters (your "useless Rc references")

You're right to flag this. The idiom rule: **take `&Rc<T>` only if the function
needs to `.clone()` the `Rc` to keep a share of ownership. If it only calls
methods, take `&T`.** Taking `&Rc<NodeRegistry>` to then just call
`registry.method()` is a useless extra layer of indirection that needlessly couples
every caller to the `Rc`.

Verified offenders — every `&Rc<NodeRegistry>` in these two files, **none of which
clone the Rc**:

```rust
// mutation_observer.rs:245  — never clones, just reads
pub(super) fn has_pending(registry: &Rc<NodeRegistry>) -> bool {
    registry.mo_entries.borrow().iter().any(|e| !e.pending.is_empty())
}
// tree/mutation.rs:352  — never clones, just calls register()
fn node_ids(registry: &Rc<NodeRegistry>, nodes: &[NodePtr]) -> Vec<u32> { ... }
```

Counted `registry.clone()` calls among the `&Rc<NodeRegistry>`-taking functions:
- `mutation_observer.rs`: **0** → every one should be `&NodeRegistry`
- `tree/mutation.rs`: **0** → every one should be `&NodeRegistry`
- `node_create.rs`: **1** (`create_js_node`, which legitimately stores a clone in
  `NodeData`) → that one keeps `&Rc<…>`.

**Recommendation:** change the signatures in `mutation_observer.rs` and
`tree/mutation.rs` from `registry: &Rc<NodeRegistry>` to `registry: &NodeRegistry`.
Callers already hold an `Rc`, and `&rc` auto-derefs, so most call sites need no
change. Net effect: looser coupling, and the signature now tells the truth about
what the function does (borrows, doesn't share). Clippy's
`needless_pass_by_ref_mut` / manual review both catch this; consider a
`clippy::ptr_arg`-style habit.

---

## PR-3 — Clone churn: mostly cheap, a few real

283 `.clone()`s, concentrated in `tree/mutation.rs` (40), `blitz_document.rs` (40),
`node_create.rs` (24), `selectors/query.rs` (23). The important distinction:

- **`NodePtr::clone()` = `Rc::clone` = one refcount increment.** Cheap. The
  hot-path counts above are mostly this. Not worth chasing for performance.
- **Real allocations worth a look:** `el.children.clone()` (clones a `Vec<NodePtr>`,
  `dom/shadow.rs`, `tree/mutation.rs:625,648`), `attrs.clone()` /
  `el.attributes.clone()` in arena construction (`stylo_bridge/arena.rs:153`,
  a full `BTreeMap<String,String>` deep clone per element), and `String`/`to_string`
  clones in tight loops.

**Recommendations:**
- Where a clone is an `Rc` bump, **leave it** — rewriting to borrows fights the
  shared-ownership DOM design and isn't worth the lifetime gymnastics.
- For the genuine deep clones (attribute maps, children vecs), prefer borrowing or
  moving where the source is no longer needed. The arena builder
  (`arena.rs:push_node`) deep-clones every element's attribute map; since it
  consumes the DOM to build a parallel arena, see whether attributes can be moved or
  referenced via `Rc<str>`/`Atom` instead of `String` copies.
- Stylistically, make `Rc::clone(&x)` explicit (vs `x.clone()`) at share points so
  refcount bumps read differently from deep clones — a common Rust convention that
  makes PR-3's "cheap vs real" distinction visible at the call site.

Minor redundancy spotted: `node_ids` does `.iter().cloned().into_iter()` — the
`.into_iter()` is a no-op after `.cloned()`. And `clone_node(c, true)` is mapped
over children in two spots (`tree/mutation.rs:625,648`) — fine, just flagging the
deep-recursive-clone cost on large subtrees.

---

## PR-4 — `RefCell` borrow panics are a runtime surface (132 uses)

`NodePtr = Rc<RefCell<Node>>` (`dom/node.rs:9`) plus 132 `RefCell`s means
`borrow()`/`borrow_mut()` aliasing is enforced at *runtime*. A re-entrant access
(e.g. a mutation callback that touches a node already `borrow_mut`'d up the stack)
panics — and per PR-1b, if that happens inside a V8 callback it's a hard failure.
This is inherent to the shared-mutable DOM design and not worth re-architecting now,
but two cheap mitigations:

- Prefer `try_borrow`/`try_borrow_mut` at known re-entrancy risk points (anything
  reachable from JS during mutation delivery) and handle the `Err` instead of
  panicking.
- Keep borrow scopes short — bind `let b = node.borrow();` for the minimum span,
  never hold one across a call back into JS or into another node's borrow.

---

## PR-5 — 70 `#[allow(dead_code)]`: decide, don't suppress

70 suppressions (10 in `layout/box.rs`, 9 in `blitz_document.rs`, 8 in
`registry.rs` alone) is a smell that the codebase carries a lot of speculative or
stranded API. Each `#[allow(dead_code)]` is a deferred decision. During the
legacy→blitz migration noted in your project memory, dead code is actively harmful:
it's surface you might "fix" or migrate that nothing actually calls.

**Recommendation:** do one pass to bucket them: (a) genuinely-future API → leave with
a one-line `// kept for X` comment; (b) legacy-path code being replaced by blitz →
delete it as part of the migration; (c) truly unused → delete now. A `cargo +nightly
udeps` / coverage pass helps confirm bucket (c). The win is the migration gets
smaller and the `#[allow]` count becomes a real progress metric.

---

## PR-6 — Lossy `as` casts on ids and lengths (low, but cheap to fix)

Pervasive `len() as i32` and `node_id as i32` / `as u32` / `as usize`
(`node_create.rs:43,433,679,807,815`, `mutation_observer.rs:387,468`, …). V8's
`Array::new`/`Integer::new` want `i32`, so the casts are somewhat forced, but:

- `usize → i32` silently truncates above `i32::MAX`. Not reachable today (no DOM has
  2³¹ nodes), so this is hygiene, not a live bug.
- `int32_value(scope).unwrap() as u32` (`node_create.rs:807`) round-trips a
  JS-supplied id through `i32`; a negative or out-of-range JS value wraps silently.

**Recommendation:** centralize id↔v8 conversion in one helper with a debug assert
(`debug_assert!(n <= i32::MAX as usize)`), and use `u32::try_from(...)` for the
inbound JS-id case so a bad id is rejected rather than wrapped. Low priority.

---

## What I would *not* spend time on

- Rewriting `Rc<RefCell<Node>>` into an arena/index DOM **now** — that's the
  blitz-dom migration's job; doing it twice is waste.
- Chasing `Rc::clone` refcount bumps for performance — not your bottleneck.
- The bulk of `expect(...)` calls — they carry messages and read fine.

## Suggested order

1. **PR-1b** — make V8 callbacks panic-free on hostile input (correctness).
2. **PR-2** — `&Rc<NodeRegistry>` → `&NodeRegistry` in the two files (mechanical, safe, immediate clarity win).
3. **PR-5** — dead-code bucketing, folded into the blitz migration.
4. **PR-1a / PR-3 / PR-6** — helper-collapse unwraps, trim real deep clones, centralize casts (opportunistic).
5. **PR-4** — `try_borrow` at re-entrancy points (as you touch them).

## Tooling recommendation

Adopt **Clippy in CI** with `-W clippy::pedantic` reviewed (not blanket-denied):
it flags PR-2 (`ptr_arg`), PR-3 (`redundant_clone`), PR-6 (`cast_possible_truncation`),
and the redundant-iterator chains automatically. Pair with `cargo +nightly fmt
--check`. That converts most of this report into an enforced lint rather than a
recurring manual audit — the highest-leverage single action here.

*Generated as part of a Rust code-quality review of Aurora.*
