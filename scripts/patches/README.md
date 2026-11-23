# Vendored crate patches

Aurora builds against patched copies of a few crates. The pristine source is
the crates.io release; Aurora's changes live here as one `git format-patch`
series per crate, in `<crate>-<version>/`. The layout follows
[kitesurf](https://github.com/cloudflare/kitesurf/tree/main/scripts/patches).

`scripts/vendor_patches.py apply` (or `make vendor`) downloads each release,
checks its sha256 against the crates.io index, commits it to a git repository
in `target/vendor-patched/<crate>-<version>/` and tags it `vendor-base`, then
applies the series with `git am`. `[patch.crates-io]` in `Cargo.toml` points
at those trees. A tree whose series is unchanged is left alone, so running
`apply` before every build is cheap.

| Series | Patches | Why |
|---|---|---|
| `blitz-dom-0.3.0-beta.4` | 5 | Selectors see through the synthetic shadow container; late stylesheet loads for removed nodes; message receiver survives a caught handler panic; two blank lines in `events/driver.rs` carried over from the fork; text and comment nodes read as an empty layout instead of panicking |
| `blitz-paint-0.3.0-beta.4` | 1 | `<body>`'s overflow propagates to the viewport (CSS Overflow 3 §3.3) |
| `stylo-0.22.0` | 1 | An unstyled element is not `display: none` (stylo#387) |

Each patch's commit message says why it exists, how it is tested, and when to
drop it. `scripts/vendor_patches.py status` lists the series and the newest
release of each crate.

## Changing a patch

```bash
scripts/vendor_patches.py apply
# edit target/vendor-patched/blitz-dom-0.3.0-beta.4/src/...
cargo build
git -C target/vendor-patched/blitz-dom-0.3.0-beta.4 commit -a
scripts/vendor_patches.py regen blitz-dom
git diff -- scripts/patches/
```

Each commit after `vendor-base` becomes one numbered patch, so keep them
atomic; `git rebase -i vendor-base` in the tree to amend, split, squash or
reorder. `regen` refuses to run with uncommitted changes, and it rebuilds a
scratch tree from the new series and compares it with the working tree before
replacing the series, so a series that does not reproduce the tree is never
written.

`apply` refuses to rebuild a tree that has uncommitted changes or commits not
yet written to the series, since rebuilding would discard them; `--force`
overrides. `cargo clean` deletes the trees; run `apply` again afterwards.

## Moving a crate to a new release

1. `git mv scripts/patches/<crate>-<old> scripts/patches/<crate>-<new>` and
   update the path in `[patch.crates-io]` and the version in `Cargo.toml`.
2. `scripts/vendor_patches.py apply <crate>`. If a patch no longer applies,
   resolve it in the tree (`git am --continue`, or `git am --skip` for a patch
   upstream made redundant).
3. `scripts/vendor_patches.py regen <crate>`.
4. Update the table above, and run the library tests and `make yt-scorecard`.
