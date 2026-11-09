# Local Blitz Paint patch

Source: crates.io `blitz-paint` 0.3.0-alpha.5, copied from the locked dependency.
The upstream license and repository are recorded in Cargo.toml.

`src/render.rs`: propagate `<body>`'s `overflow` to the viewport (CSS Overflow
§3.3). When the root element is `overflow: visible`, body must not clip its
content. Upstream treats a `body { overflow-y: scroll }` (YouTube) as a
clipping scroll container; when body's only in-flow-relevant child is
absolutely positioned its box is 0px tall, so every non-fixed descendant was
clipped away and the page painted blank below the fixed masthead.

Remove this fork when the dependency contains the equivalent fix.
