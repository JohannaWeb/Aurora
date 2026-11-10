# Local Blitz Paint patch

Source: crates.io `blitz-paint`, at the version recorded in `UPSTREAM`. The upstream
license and repository are recorded in Cargo.toml. To move to a newer release, run
`scripts/update_forks.py update blitz-paint <version>` and resolve any conflicts it
reports; `scripts/update_forks.py diff blitz-paint` prints the patch below.

`src/render.rs`: propagate `<body>`'s `overflow` to the viewport (CSS Overflow
§3.3). Upstream (since 0.3.0-beta.4) stops the root element from clipping its own
overflow, but still treats a `body { overflow-y: scroll }` (YouTube) as a clipping
scroll container. When body's only in-flow-relevant child is absolutely
positioned its box is 0px tall, so every non-fixed descendant was clipped away
and the page painted blank below the fixed masthead. The patch combines both:
the root never clips, and body does not clip when the root is `overflow: visible`.

Remove this fork when the dependency contains the equivalent fix.
