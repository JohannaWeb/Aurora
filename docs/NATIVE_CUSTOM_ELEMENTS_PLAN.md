# Native custom-element reactions + slot composition

*2026-06-22. A plan to move the custom-element lifecycle and shadow-DOM
composition from external JS shims into the native Rust DOM, using Ladybird's
`LibWeb/DOM/Node.cpp` insertion algorithm as the reference. Companion to
`docs/YOUTUBE_HYDRATION_INVESTIGATION-2026-06-18.md` (the narrative of why
YouTube content doesn't render) — this is the structural answer to it.*

---

## The core inversion

Today Aurora's custom-element lifecycle is **JS-driven from the outside**:
`src/js_polyfills/custom_elements.js` (~2300 lines) owns the registry, the
upgrade stack, and reaction firing, and Rust pokes it from the outside via
`apply_polymer_bindings` / `drive_polymer_page_manager_navigation` in
`src/runner/pipeline.rs`. Connection is a best-effort *sweep*
(`__aurora_sweep_bindings__`) that races YouTube's minified bundle.

Ladybird's lifecycle is **DOM-driven from the inside**: the *insertion
algorithm itself* (`LibWeb/DOM/Node.cpp::insert`, steps 7.x) enqueues
`connectedCallback` for every connected custom element it touches, and a
microtask drains the queue. Nothing external drives it — connection falls out of
mutation, correctly ordered relative to siblings and shadow descendants.

**Aurora already has the exact choke point Ladybird hooks into.** Every JS DOM
mutation funnels through `apply_dom_mutation` in `src/js_v8/tree/mutation.rs`.
That function *is* Aurora's `Node::insert`. The whole plan is: make
`apply_dom_mutation` do natively what `Node.cpp:630-748` does.

This is a load-bearing wall. Nearly every YouTube blocker in the project memory
log traces back to lifecycle/composition being external:

- `ytd-app.connectedCallback` never firing reliably (2026-06-20 (4))
- ShadyDOM logical-root adoption / "78 orphaned fragments" (2026-06-20 (3))
- mirror drift from re-stamping (`legacy_node_key = Rc::as_ptr`) (2026-06-20 later)
- event delivery faults (2026-06-14)

Doing this collapses several of them at once. It does **not**, by itself, solve
the closure-private navigation blocker (`o7.navigate`, the T2 transition) — but
that is downstream: once `connectedCallback` fires natively and reliably,
`ytd-app` connects and its own bootstrap navigation has a real chance to run
without the external nav driver. That is the hypothesis to validate at Phase 5,
not assume.

---

## Reference: Ladybird's three moving parts

### 1. Per-element reaction queue + agent-level stack (`CustomElementReactionsStack.h`)

Each element has a FIFO `custom_element_reaction_queue` of pending reactions
(upgrade or callback). The agent holds an `element_queue_stack` (one queue per
CEReactions boundary) plus a `backup_element_queue` drained by a microtask.

### 2. Enqueue (`Element.cpp:3306` / `:3354`)

- `enqueue_a_custom_element_callback_reaction(name, args)` — look up the callback
  on the element's definition, filter (the `observedAttributes` check for
  `attributeChangedCallback`), push onto the element's reaction queue, then
  *enqueue the element on the appropriate element queue*.
- `enqueue_an_element_on_the_appropriate_element_queue` — if the stack is empty,
  push to the backup queue and (if not already) queue a microtask to drain it;
  otherwise push onto the current element queue.

### 3. Invoke (`MainThreadVM.cpp:781`)

`invoke_custom_element_reactions(queue)` — drain the element queue; per element,
drain its reaction queue, dispatching `Upgrade` (call constructor with the
existing element as `this`) or `Callback` (invoke the JS function).

### 4. The insertion algorithm wires it (`Node.cpp:630-748`)

```
for each node_to_insert:
  adopt into document
  append/insert into children          (list mutation)
  if named shadow host & slottable: assign_a_slot
  assign_slottables_for_a_tree(root)   (slot composition)
  for each shadow-including inclusive descendant:
      run insertion steps
      if not connected: continue
      if custom: enqueue connectedCallback
      else:       try_to_upgrade
children_changed
post_connection (after collecting a static list)
```

---

## How it maps onto Aurora

### Storage that already exists

- `src/js_v8/tree/mutation.rs::apply_dom_mutation` — the choke point. Variants
  `AppendChild` / `PrependChild` / `InsertBefore` / `RemoveChild` /
  `ReplaceChild` / `SetAttribute` / `AttachShadow` already funnel every JS
  mutation.
- `src/dom/node.rs::ElementNode` already carries `shadow_root: Option<NodePtr>`,
  `template_contents: Option<NodePtr>`, `assigned_nodes: Vec<NodePtr>`, and a
  `parent` back-pointer. The *storage* for slot composition is present; nothing
  populates `assigned_nodes` natively.
- `src/js_v8/registry.rs::NodeRegistry` already stores `v8::Global<v8::Object>`
  wrappers and `v8::Global<v8::Function>` listeners in `RefCell<BTreeMap<…>>`,
  so it already handles V8-global drop ordering correctly. It is the right home
  for native custom-element state.
- `mutation::is_connected_to` (used at `node_create.rs:2527`) == Ladybird's
  `is_connected()`.

### What's new

```rust
// src/js_v8/custom_elements.rs (new)

enum CeState { Undefined, Uncustomized, Custom, Failed }

struct CeDefinition {
    name: String,
    constructor: v8::Global<v8::Function>,
    connected: Option<v8::Global<v8::Function>>,
    disconnected: Option<v8::Global<v8::Function>>,
    attribute_changed: Option<v8::Global<v8::Function>>,
    observed_attributes: HashSet<String>,
}

enum Reaction {
    Upgrade  { definition: Rc<CeDefinition> },
    Callback { cb: v8::Global<v8::Function>, args: Vec<v8::Global<v8::Value>> },
}

// agent-level, on NodeRegistry:
struct CeRegistry { definitions: RefCell<BTreeMap<String, Rc<CeDefinition>>> }
struct ReactionsStack {
    element_queue_stack: RefCell<Vec<Vec<u32>>>,   // CEReactions boundaries (node ids)
    backup_queue: RefCell<Vec<u32>>,
    processing_backup: Cell<bool>,
}
// per-element reaction queue: side-table keyed by node id on NodeRegistry,
// to avoid bloating the Node enum.
```

The **callbacks stay JS** (`v8::Global<v8::Function>`), exactly as Ladybird keeps
them `WebIDL::CallbackType`. Only the *queue, ordering, and scheduling* move
native.

### The insertion steps inside `apply_dom_mutation`

For `AppendChild` / `PrependChild` / `InsertBefore`, between the list mutation
and the render sync, mirror `Node.cpp:674-714`:

```rust
append_child_ptr(parent, child);                       // list + parent link
assign_slottables_for_tree(registry, &root_of(parent)); // step 7.6 (Phase 4)
for desc in shadow_including_inclusive_descendants(child) {
    run_insertion_steps(registry, &desc);
    if !is_connected_to(&registry.document, &desc) { continue; }
    if desc.is_element() {
        if ce_state(&desc) == CeState::Custom {
            enqueue_callback_reaction(registry, &desc, CONNECTED, vec![]);
        } else {
            try_to_upgrade(registry, &desc);
        }
    }
}
```

Symmetrically: `RemoveChild` → `disconnectedCallback`; `SetAttribute` →
`attributeChangedCallback` (the observed-attrs filter inside the enqueue drops
unobserved names). Reactions drain at the microtask checkpoint inside the
existing `pump_ready_work` virtual-clock loop.

### Native slot composition (Phase 4)

Port `LibWeb/DOM/Slottable.cpp` + `Slot.cpp`:

- `assign_a_slot(slottable)` — find the matching `<slot>` in the host's shadow
  root (by `slot` attribute, else the default slot); append to its
  `assigned_nodes`. (`Node.cpp:643-650`)
- `assign_slottables_for_a_tree(root)` — recompute assignments after a mutation.
- `flattened_children(node)` — the composed view Blitz consumes instead of raw
  `children`: at a shadow host recurse into `shadow_root`; at a `<slot>` expand
  to `assigned_nodes` (falling back to the slot's own children).

`sync_inserted_nodes_to_render_document` switches from walking `el.children` to
walking `flattened_children`. This retires the hand-rolled
`adoptLogicalShadowRoot` / `__aurora_fragment_owner__` machinery and collapses
the `Rc::as_ptr` mirror-drift problem, because the composed tree becomes the
single authority for what Blitz mirrors.

---

## What stays in JS vs. moves native

| Concern | Today | Proposed |
|---|---|---|
| `customElements.define` registry | JS (`custom_elements.js`) | **Native** `CeRegistry` |
| Upgrade / reaction scheduling & ordering | JS upgrade stack + external sweeps | **Native** reaction queue + element-queue stack + backup queue |
| Reaction *callbacks* (constructor, `connectedCallback`, …) | JS | **JS** (invoked from native, like `CallbackType`) |
| Slot assignment / flattened tree | JS ShadyDOM shims | **Native** `assign_slottables` + `flattened_children` |
| Connectivity / insertion trigger | external `apply_polymer_bindings` | **Native**, inside `apply_dom_mutation` |

`custom_elements.js` shrinks to a thin binding layer: `define` reads
`observedAttributes` / prototype callbacks once and registers a native
`CeDefinition`. The ~1800-line upgrade/sweep/composition machinery is deleted.

---

## Phasing (each independently shippable; one purpose per PR)

1. **Native `CeRegistry` + `define` binding.** Definitions mirror into a native
   registry; JS still drives upgrade. No behavior change — pure plumbing.
   Verified by a unit test that `customElements.define` populates the native
   registry. **← this PR.**
2. **Reaction queue + microtask backup drain.** Add the spine; wire
   `connectedCallback` enqueue into `AppendChild`/`InsertBefore` only, behind a
   flag, A/B against the JS sweep on the existing `runtime_tests.rs` CE tests.
3. **`disconnectedCallback` + `attributeChangedCallback`** in `RemoveChild` /
   `SetAttribute`.
4. **Native slot composition + `flattened_children`** feeding Blitz; retire the
   ShadyDOM logical-root shims.
5. **Delete the external drivers** (`apply_polymer_bindings`,
   `drive_polymer_page_manager_navigation`) once 1–4 carry YouTube past
   `ytd-app.connectedCallback` on their own. Validate the navigation hypothesis.

### Measurement rig (unchanged)

`AURORA_DEBUG_YOUTUBE=1 AURORA_HEADLESS=1 cargo run -- \
"https://www.youtube.com/results?search_query=rust+programming"` — the clean,
data-rich, no-Stylo-panic page. `AURORA_DEBUG_RENDER=1` for paint-path counts;
`AURORA_TRACE_CE` for adopted/unresolved root counts.

---

## Risks

- This is a multi-week structural change, not a fix — effectively rebuilding the
  custom-elements + shadow-DOM core the Ladybird way. Justified because it is the
  load-bearing wall under several independent blockers.
- The navigation blocker is downstream and only *hypothetically* unblocked by
  this. Phase 5 validates it; don't assume.
- Drop ordering: all `v8::Global` handles must dispose before the isolate (see
  the project's isolate-drop-order note). Hanging CE state on `NodeRegistry`
  inherits its existing, correct ordering.
