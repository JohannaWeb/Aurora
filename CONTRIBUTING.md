# Contributing to Aurora

Aurora is an experimental browser engine written in Rust. It is early-stage, opinionated, and moving fast. Contributions are welcome if you understand what you are getting into.

---

## What kind of project is this

Aurora is not a hobbyist project and not trying to become a general-purpose browser. It is a serious attempt to build a sovereign, GPU-accelerated browser engine from the ground up in Rust, aligned with the Bastion stack. The long-term differentiator is capability-gated identity and decentralized protocol support — not just another Chromium wrapper.

That means some decisions that look like shortcuts are intentional. Survival stubs exist on purpose. Some Web APIs will remain partial forever if full compliance is not load-bearing for the goal.

Read [`docs/current/IMPLEMENTATION_PLAN.md`](docs/current/IMPLEMENTATION_PLAN.md) and the phase files before pitching large features.

---

## Ways to contribute

- **Bug reports** — open an issue using the bug report template
- **Phase work** — pick up a checked item from a phase file; most phases have clear open tasks
- **Test coverage** — new unit tests and snapshot additions are always useful
- **Documentation fixes** — if something in the docs is wrong or misleading, fix it

If you want to work on something significant, open an issue first so the work is not duplicated.

---

## Setting up the environment

### Option 1 — GitHub Codespaces (recommended)

The repo ships a devcontainer. Click **Code → Codespaces → New codespace** on GitHub. All system dependencies (Wayland/X11 dev headers, pkg-config, Rust toolchain) are installed automatically.

### Option 2 — Local Linux

You need:

- Rust stable (1.80+)
- The system packages installed by the devcontainer:

```bash
sudo apt-get install -y \
  libwayland-dev libx11-dev libx11-xcb-dev libxcb1-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev pkg-config
```

### Option 3 — macOS / Windows

wgpu and winit support both platforms. You do not need the Wayland packages above. A GPU capable of Vulkan, Metal, or DX12 is required for the windowed renderer.

---

## The Opus dependency

Aurora depends on a sibling crate called `Opus` at the path `../Opus`. When working in Codespaces or inside the repository checkout alone this path will not resolve.

**To work around this locally**, clone the parent organization repo which contains both crates as siblings, or use a `[patch.crates-io]` override in your local Cargo config. Most contributors working in Codespaces do not hit this because the devcontainer environment is pre-configured.

If you are contributing and hit the missing `Opus` error, open an issue — it is a known friction point and will be resolved as the public repository structure is settled.

---

## Building and running

```bash
# Build
cargo build

# Run with the default demo fixture
cargo run -- --fixture demo

# Fetch a live page
cargo run -- https://example.com/

# Debug flags
cargo run -- --fixture google-homepage --debug-dom --debug-style --debug-layout
```

The windowed renderer requires a display. In headless CI or Codespaces without X forwarding, use screenshot mode instead:

```bash
make screenshot FIXTURE=demo
```

---

## Testing

```bash
# Unit and integration tests
cargo test

# Visual regression tests (compares against stored snapshots)
make check-snapshots

# Regenerate snapshots after an intentional visual change
make update-snapshots
```

`cargo test` must pass before any PR is merged. Visual regression failures need to be explained in the PR description.

---

## Code conventions

### File size cap

**Source files must not exceed 200 lines.** This is enforced by the Makefile:

```bash
make check-line-cap
```

The cap is deliberate — it keeps modules focused. If a file is growing beyond 200 lines, split it into a module directory with logical sub-files, following the pattern used throughout `src/`.

### PR size limit

**Pull requests must be 400 lines or fewer** (diff lines, not counting generated files or lock files). Larger changes should be broken into a series of stacked PRs.

### Comments

Do not explain *what* the code does — well-named identifiers already do that. Only add a comment when the *why* is non-obvious: a spec constraint, a workaround for a known upstream bug, or a subtle invariant. No multi-line comment blocks.

### Error handling

Do not add fallbacks or error handling for scenarios that cannot happen. Trust internal guarantees. Validate only at system boundaries (user input, network responses, external APIs).

### No speculative features

Do not add abstractions, generics, or configuration for hypothetical future requirements. Implement exactly what the current phase task describes.

---

## Where help is most useful

Check the phase files in [`docs/current/phases/`](docs/current/phases/) for open checkboxes. The most actionable areas right now:

| Area | Phase file | Example open tasks |
|---|---|---|
| CSS media query evaluation | [PHASE_2](docs/current/phases/PHASE_2.md) | Evaluate `max-width` conditions against viewport |
| CSS `@keyframes` | [PHASE_2](docs/current/phases/PHASE_2.md) | Parse and ignore initially, unblock later |
| Layout invalidation | [PHASE_5](docs/current/phases/PHASE_5.md) | Per-node dirty tracking |
| Visual regression CI | [PHASE_8](docs/current/phases/PHASE_8.md) | Snapshot diffing in CI pipeline |
| JS timer scheduling | [PHASE_9](docs/current/phases/PHASE_9.md) | `clearTimeout` after fire, idle callback semantics |

If you want to pick up a task, comment on the relevant issue or open a new one so work is not duplicated.

---

## Submitting a pull request

1. Fork the repo, create a branch from `main`.
2. Keep the PR under 400 diff lines. Stack smaller PRs if needed.
3. Make sure `cargo test` passes and `make check-line-cap` is clean.
4. Fill in the PR template — the phase reference and test plan fields are required.
5. Link to the issue or phase task the PR closes.

First-time contributors: a small PR fixing a bug or adding a test is a good starting point.

---

## License

By contributing you agree your contribution is licensed under the [Mozilla Public License 2.0](LICENSE) that covers this project.
