# The Roast (Productive Edition)

*A principal-browser-engineer-shaped reading of Aurora, as of branch
`javascript/move-to-spider-monkey-rust-bindings`. Written with love. The kind
of love that tells you there's spinach in your teeth before the demo.*

The deal: every burn comes with a fix. A roast that doesn't ship a patch is
just a code review with worse manners.

---

## TL;DR

Aurora is a genuinely impressive solo browser engine that is currently running
**three rendering stacks, two DOMs, and a JS engine wearing another JS engine's
name tag**. The engineering instincts are good. The problem is you keep all your
old instincts in the repo too. You don't have a browser engine; you have an
archaeological dig of every browser engine you've ever started.

Headline numbers:

- **5,617 lines** of `src/js_boa/` — not declared as a module anywhere. Dead.
- **825 lines** of `src/stylo_bridge/` — not declared as a module anywhere. Dead.
- **3 layout/render paths**: hand-rolled (`css`+`style`+`layout`), `blitz-dom`/`blitz-paint`, and the orphaned `stylo_bridge`.
- **112 `.unwrap()` calls** in software whose entire job is eating hostile input.
- A JS module named `js_sm` that exports `SmRuntime as BoaRuntime` so it can lie to the other 200 call sites.

---

## 1. The engine has an identity crisis, and it's load-bearing

```rust
// src/js_sm/mod.rs
pub use runtime::SmRuntime as BoaRuntime;
```

You migrated from Boa to SpiderMonkey. Bravo — that's the right call, Boa was
never going to run the real web. But the migration strategy was "rename
SpiderMonkey to Boa so I don't have to touch the call sites." The whole pipeline
still says `crate::js_sm::BoaRuntime`. The README still proudly advertises an
"embedded **Boa**-based JavaScript runtime." The branch is named
`move-to-spider-monkey`. The struct is `SmRuntime`. The alias is `BoaRuntime`.
The directory is `js_sm`. The *old* directory `js_boa` is still sitting there,
5,617 lines deep, like an ex's stuff in your garage.

A future contributor (or future you, in three weeks) will `grep BoaRuntime`,
land in SpiderMonkey, and lose an afternoon to the question "wait, which engine
am I in?"

**Fix:** Rename `BoaRuntime` → `SmRuntime` at the call sites. It's a mechanical
find-and-replace and it buys you permanent clarity. Delete `src/js_boa/`
entirely — it's not compiled, git remembers it, and it is actively misleading
every search you'll ever run. Update the README to say SpiderMonkey. Your engine
should know its own name.

## 2. You have a `MAX_SCRIPT_BYTES` limit justified by a fact that's no longer true

```rust
// Scripts larger than this are skipped: Boa has no JIT, so multi-MB bundles
// take minutes to interpret and always fail on modern syntax anyway.
const MAX_SCRIPT_BYTES: usize = 256 * 1024;
```

```rust
eprintln!("Boa: skipping {} ({} KB, over {}KB limit — Boa has no JIT)", ...);
```

SpiderMonkey has one of the best JITs ever written. You just spent a whole
branch wiring it in. And the code is *still* turning real bundles away at the
door citing the limitations of the engine you just deleted. The two duplicate
commits literally titled `Boa still cant support youtube sadly` are the
tombstone of the old engine — but the gravestone is now nailed to the new one.

**Fix:** Re-evaluate the budget now that you have a JIT. Maybe it stays for
memory/time safety reasons (legitimate!), but the *justification* in the comment
and the user-facing error are wrong. Bad rationale is worse than no rationale
because it stops the next person from questioning the limit.

## 3. The reflow path is a full page reload in a trench coat

This is the one that made me put down my coffee.

```rust
// src/window/input.rs — on every JS DOM mutation:
let html = crate::js_sm::serialize_outer_html(&self.dom);
*blitz_doc = BlitzDocument::from_html(&html, ...);
```

When JavaScript does `el.style.color = 'red'`, here is what happens:

1. The entire legacy DOM is **serialized back into an HTML string**.
2. A **brand-new `BlitzDocument` is constructed from scratch** by re-parsing that string.
3. Blitz re-runs full document construction and layout from zero.

That's O(whole document) per mutation. It throws away every scrap of
incremental layout blitz gives you for free. It nukes all blitz-side state on
every keystroke: scroll position, focus, in-progress form input, hover, any
fetched-but-not-yet-arrived resources, the lot. An animation loop calling this
every frame is reparsing your HTML 60 times a second.

This isn't a reflow. A reflow recomputes geometry. This is `document.write` of
the entire page, every time anything changes. The comment even calls itself out:
`6f65c5c Reflow and remove old docs. Correction reflow isnt over` — you already
know.

**Fix:** This is the single highest-leverage change in the repo. Pick *one* DOM
to be canonical and mutate blitz-dom in place through its real mutation API
instead of serialize→reparse. Which brings us to the actual root cause...

## 4. Two DOMs that must agree, kept in sync by stringifying one into the other

Right now:

- **Legacy DOM** (`dom::NodePtr`) is the source of truth for JS, for tests, and
  for hit-testing node identity on click.
- **blitz-dom** is the thing that actually paints pixels.
- They are reconciled by serializing legacy → HTML → reparsing into blitz.

So **hit-testing runs in the legacy `LayoutTree`** and **painting runs in
blitz's layout** — two different layout engines that are free to disagree about
where any given box lives. The moment they diverge by a pixel, your click
targets stop matching what the user sees. You've built a browser where "what you
see" and "what you click" are computed by different engines connected by a CSV
export.

`src/window/app_handler.rs` confirms it: blitz does the anchor hit-test for
navigation, but legacy `LayoutTree` does the hit-test for JS event-target node
identity. Two sources of truth for "what is under the cursor."

**Fix:** Commit to blitz-dom as *the* DOM. It already does parsing, styling
(stylo!), layout, hit-testing, and painting — the things your hand-rolled
`css`/`style`/`layout` stack does partially. Expose the JS bridge over
blitz-dom's node IDs (you're already using `__node_id` indirection — perfect fit).
Keep the hand-rolled layout *only* as a test oracle if you must, behind the
`taffy-document` feature, clearly labeled "reference implementation, not in the
render path." One DOM. One layout. One hit-test.

## 5. `stylo_bridge/` — 825 lines of code that the compiler has never met

```
$ grep -rn "mod stylo_bridge" src   # (empty)
```

It's not declared. It doesn't build. It does, however, show up in every grep,
every IDE symbol search, and every "how does styling work here?" investigation.
There's an `arena.rs` with unsafe code in it that nothing exercises. Dead unsafe
code is the worst kind of dead code: it looks important enough that nobody dares
delete it, so it metastasizes.

**Fix:** Delete it. If stylo-via-blitz is the future (it should be — that's what
blitz-dom *uses*), then this hand-rolled bridge is a fork of a fork of a
direction you abandoned. `git rm -r src/stylo_bridge`. Git is your archive.
The `src/` tree is not.

## 6. 112 `.unwrap()`s in the most adversarial software a person runs

A browser is the one program where literally every byte of input is chosen by a
stranger who may actively want to hurt you. `handles.into_iter().map(|h| h.join().unwrap_or(None))`
is fine. But 112 of these across fetch, parse, and JS bridge paths means there
are 112 strings on the internet that crash Aurora. Each one is a free remote DoS.

**Fix:** You don't have to fix all 112 today. Do triage: any `.unwrap()` on a
path that touches network bytes, HTML, CSS, or JS-supplied values gets converted
to a real error or a logged-and-recovered fallback. Add
`#![warn(clippy::unwrap_used)]` to the fetch/parse/js modules so new ones can't
sneak in. A browser that "survives hostile input" (your README's words) can't
have 112 panic landmines.

## 7. The differentiator is real but underbuilt

Your stated edge — and I think it's a *good* one — is capability-gated browsing:

```rust
// file:// only allowed when identity has workspace.read
```

That's a genuinely interesting thesis: a browser as a sandboxed, capability-aware
agent in a "sovereign stack." But right now the capability model is one check on
one scheme. Network fetch isn't gated the same way. There's no capability story
for storage, for JS-initiated fetch, for cross-origin, for `file://` *write*.
The thing that makes Aurora *Aurora* is currently a single `if`.

**Fix:** If capability-gating is the differentiator, it deserves to be an
architecture, not a guard clause. Define the capability set
(`network.fetch`, `network.host(...)`, `storage.local`, `workspace.read/write`,
`script.eval`) and thread `Identity` through *every* boundary that touches the
outside world, with the same rigor you'd give a real sandbox. That's a moat
Chromium can't trivially copy because it'd break the whole web. Lean into it.

---

## Credit where it's due (a roast is not a hit piece)

You did several things that a lot of "I'm writing a browser" projects get wrong:

- **You adopted blitz-dom instead of hand-rolling layout into the heat death of
  the universe.** Knowing when to stop building your own is senior judgment.
- **Real TLS validation.** You didn't `danger_accept_invalid_certs`. The number
  of toy browsers that skip cert validation "for now" (forever) is staggering.
- **GPU-first via Vello/WGPU.** That's the modern, correct rendering bet, not a
  software rasterizer you'll have to throw away.
- **Parallel external-script fetch with order preservation**, plus a net cache
  in the blitz provider. Thoughtful.
- **The `__node_id` registry indirection** so parser/renderer/JS see mutations
  through one handle is exactly the right pattern — it'll make collapsing to one
  DOM (#4) much easier than it would otherwise be.
- **You migrated JS engines at all.** Ripping out Boa for SpiderMonkey is a big,
  correct, unglamorous infrastructure call. Most people would've kept polishing
  the engine that can't load YouTube.

---

## The one-paragraph version for your standup

Aurora's bones are good and its bets are mostly right, but it's carrying three
layout stacks, two DOMs, and ~6,400 lines of dead code, and its "reflow" is a
full HTML reparse on every mutation. The fix isn't more features — it's
*subtraction*: delete `js_boa` and `stylo_bridge`, rename `BoaRuntime` to the
truth, make blitz-dom the single canonical DOM, and turn the
serialize→reparse reflow into in-place mutation. Do those four deletions and
Aurora goes from "four half-browsers in a trench coat" to "one real browser
with a sharp, defensible thesis."

Now go delete some code. It's the most satisfying commit you'll write all month.
