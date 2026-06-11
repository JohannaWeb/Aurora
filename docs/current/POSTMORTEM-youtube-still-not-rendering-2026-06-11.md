# Postmortem: YouTube still not rendering, one day and two commits later

**Date:** 2026-06-11
**Status:** Open. Hydration root cause remains unconfirmed; the confirming
experiment has still not been run. New rendering-path defects were introduced and
are confirmed.
**Scope:** Commits `c6331b3` ("Added v8 and youtube battle continues") and
`d5ff88e` ("added v8 and further support"), following the 2026-06-10 postmortem
([POSTMORTEM-youtube-blank-page.md](POSTMORTEM-youtube-blank-page.md)).
**Companion doc:** [ANALYSIS-youtube-rendering-2026-06-11.md](ANALYSIS-youtube-rendering-2026-06-11.md)
holds the full code-verified technical state; this document is about what happened
and why we are still blank.

---

## Summary

Since yesterday's postmortem, four of its six contributing causes were genuinely
fixed (boot pump, `IdleDeadline`, `MessagePort` delivery, the O(document)
`customElements.define` scan), a V8 backend was scaffolded, and the renderer was
swapped to a serialize→Blitz round-trip. **None of these touched the identified
primary cause** — Polymer silently skipping template stamping in `ytd-app` — and
the single experiment that would confirm or kill that hypothesis (the
`AURORA_DEBUG_YOUTUBE=1` probe run, fully instrumented and documented as "next
step #1" in both prior docs) was never executed. There are no probe artifacts in
the repo; the `logs/` directory referenced by the old postmortem no longer exists.

The page is therefore presumed still blank for the same unproven reason as
yesterday, and would now render *incorrectly even if hydration succeeded*, because
the new serializer corrupts every JS-injected stylesheet on each reflow.

## Timeline

| When | What |
|---|---|
| 2026-06-09 → 06-10 | SpiderMonkey migration lands (PR #29). Segfault fixed (`24c1727`). Template/`currentScript`/custom-element groundwork lands. |
| 2026-06-10 | First postmortem written. Root cause narrowed (not confirmed): Polymer `_attachDom` silently skipped for `ytd-app`. Probe instrumentation wired. "Next step 1: run the probe." |
| 2026-06-11 早 (`c6331b3`) | V8 crate added (execute-only stub). Custom-elements shim substantially reworked: pending-queue upgrades, template accessor that defers to inherited static getters, `MessagePort` delivery, expanded probe logging. Probe still not run. |
| 2026-06-11 (`d5ff88e`) | Render path moved to serialize→Blitz for first paint *and* every reflow (`src/window/input.rs:71-85`). Probe still not run. |
| 2026-06-11 (this doc) | Code audit confirms: 4 of 6 old contributing causes fixed, primary cause untouched/unverified, 2 new confirmed defects introduced by the serializer. |

## What went right

- The contributing-cause fixes were real fixes, verified in code, not paper-overs:
  the boot pump (`src/runner/pipeline.rs:164-182`) removes the 108-second frozen
  boot; the define queue removes ~107 s of redundant `querySelectorAll`.
- The V8 backend is an **honest stub**: every unimplemented `JsRuntime` method
  returns inert defaults and says so in a comment (`src/js_v8/runtime.rs:90-93`).
  This is exactly the lesson the 2026-06-10 doc drew from the `MessageChannel`
  half-implementation ("feature detection passes, behaviour doesn't") — applied
  correctly the next day.
- The probe instrumentation kept improving and is now genuinely good — it walks the
  static-template chain and avoids mutating Polymer's `_template` memoization while
  observing it (`src/js_sm/globals/browser_api.rs:1489-1614`).

## What went wrong

### 1. The diagnostic loop was never closed (process failure, primary)

Both prior docs end with the same instruction: *run the probe, append the output,
then decide.* Instead, two more days of fixes were stacked on an unconfirmed
hypothesis. The fixes were individually sound, but we cannot say whether the
blank page's cause moved, because the measurement that defines "the cause" was
never taken. We are now maintaining a ranked-suspect list
(analysis doc §2) where one log file would give certainty.

### 2. The renderer swap introduced confirmed regressions in the YouTube path

The serialize→Blitz round-trip (`d5ff88e`) was motivated by a real problem
(bootstrap mutations invisible at first paint) but shipped with two defects that
specifically punish a JS-styled site like YouTube:

- **CSS corruption:** `serialize_outer_html` HTML-escapes the text children of
  `<style>` and `<script>` (`src/js_sm/serialization.rs:36-38,62-67`). Rawtext
  entities are not decoded on re-parse, so any selector containing `>` (or CSS
  containing `&`, quotes) is destroyed. YouTube injects essentially all of its CSS
  via `<style>` elements at runtime.
- **Quirks mode:** no doctype is serialized (the DOM has no doctype node), so the
  Blitz/Stylo pass renders the page in quirks mode.

Neither has a test. Both would have been caught by a golden-file test
round-tripping a `<style>a > b{}</style>` document, or by *any* visual run against
a real site — which loops back to failure #1.

### 3. Effort fragmented across engines instead of across the blocking layer

In the same window where the confirmed-blocking layer (Polymer hydration) sat
unmeasured, a third engine backend was scaffolded (V8, ~200 lines + build plumbing,
`Cargo.toml` feature, runtime tests). The stub is well made (see "what went
right"), but YouTube cannot render on it even in principle — it has no DOM bridge —
and SpiderMonkey remains the only viable engine. This continues the pattern from
the Boa era (`0a59300` "Boa still cant support youtube sadly"): engine churn is the
project's recurring displacement activity when the web-platform layer gets hard.

### 4. Documentation drift

The 2026-06-10 postmortem still presents the four fixed contributing causes as
open defects, and `YOUTUBE-whats-left.md` references log files that are no longer
in the tree. Anyone joining the effort today would re-fix fixed things. (The
companion analysis doc now carries the corrected state; the old docs are
superseded but left in place as the historical record.)

## Root cause of the *postmortem-level* failure

Not a single technical item. The operating pattern is: **hypothesize → fix
adjacent confirmed defects → re-hypothesize**, skipping the **measure** step,
because fixes feel like progress and probe runs feel like overhead. Two days in a
row, "run the probe" was the documented next step and lost to new code both times.

## Lessons / corrective actions

1. **No further hydration-adjacent changes until a probe log exists.** The run
   costs minutes; the suspect list it would collapse has now consumed two days of
   inference. Command and interpretation table are in
   [YOUTUBE-whats-left.md](YOUTUBE-whats-left.md); append output to this document.
2. **Probe artifacts belong in the repo.** Yesterday's log files
   (`logs/aurora_20260610_*.log`) are referenced by a postmortem but gone. Add
   `docs/probes/` (or commit the relevant excerpts into the postmortem itself, as
   was already the stated intent).
3. **The serializer needs tests before the next renderer change**, starting with
   rawtext round-trip and doctype emission. The Blitz path silently re-renders the
   whole document per reflow; correctness bugs there are invisible until a real
   site exercises them.
4. **Park V8 until SpiderMonkey paints YouTube.** The trait seam
   (`src/js_engine.rs:33-60`) is the right design and makes parking cheap; the
   stub loses nothing by waiting.
5. **Mark superseded docs as superseded** at the top, with a pointer forward —
   done for the 2026-06-10 pair via the companion analysis doc.

## Current single source of truth

[ANALYSIS-youtube-rendering-2026-06-11.md](ANALYSIS-youtube-rendering-2026-06-11.md)
— verified state of every layer (hydration suspects ranked, renderer defects
confirmed with file:line, platform gaps), and the ordered, falsifiable next steps.

---

*Appendix (pending, carried over from 2026-06-10 — now two days outstanding):
captured probe output for `ctor.template` / `el._template` / `el.root` on
`ytd-app` and `ytd-masthead`.*
