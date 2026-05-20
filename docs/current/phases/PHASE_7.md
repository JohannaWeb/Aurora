# Phase 7 — Networking (reqwest)

**Status: Mostly done — HTTP transport replaced; off-thread fetches not done**

Confirmed done: `src/fetch/http.rs` uses reqwest blocking + rustls. `capability.rs`, `resolve.rs`, `data_url.rs` all in place. gzip/deflate/brotli enabled.

Confirmed open: no `thread::spawn`, channels, or async anywhere in `src/fetch/`. All fetches block the main thread.

## Work items

- [x] Choose `reqwest` (blocking + rustls, `default-features = false`)
- [x] Replace bespoke HTTP transport — `src/fetch/` is now `api.rs`, `capability.rs`, `data_url.rs`, `errors.rs`, `http.rs`, `resolve.rs`, `url.rs`
- [x] Compression: gzip, deflate, brotli enabled on reqwest
- [ ] Read [`blitz-net`](https://github.com/DioxusLabs/blitz/tree/main/packages/blitz-net) end-to-end — reference for minimum glue from HTTP client to engine
- [ ] Run image fetches off the layout thread (channels back to the event loop)
- [ ] Run script fetches off the parser thread (preload scanner pattern)

## Outcome

Closes P0 #10 / P1 #20 (synchronous network on main thread). Required before Phase 5's "stop re-fetching images on resize" lands cleanly.
