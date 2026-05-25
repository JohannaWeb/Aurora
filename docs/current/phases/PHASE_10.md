# Phase 10 — Strategic Direction

**Status: Decided ✅**

## Decision

- [x] **Path A — Aurora stays independent.** Built on Blitz-aligned crates (html5ever, cssparser, selectors, Taffy, Parley, eventually AnyRender) but as its own engine under the Bastion umbrella.
- [ ] ~~Path B — converge with Blitz~~ — declined
- [ ] ~~Path C — static-only~~ — declined

## Differentiators

- Stubbed identity system with capability-gated fetch (`src/fetch/capability.rs`)
- Sovereign-runtime integration
- Bastion umbrella ownership

## Philosophy (adopted from Nico Burns / Blitz)

> "If there's a crate that implements a subsystem in a way that can be used standalone then we'll use it. But we'll also extend it / contribute to it / treat it like part of our engine rather than treating it like a black box."

Use Blitz as reference; contribute upstream only opportunistically, not as a merge strategy.
