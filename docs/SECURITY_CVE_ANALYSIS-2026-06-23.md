# Aurora Security & CVE Analysis — 2026-06-23

**Scope:** Aurora-owned Rust code in `src/` (capability model, fetch stack, V8 DOM
bridge, Stylo integration, media, packaging). Third-party engines (V8, Stylo,
blitz-*, reqwest, wgpu) are treated as trusted dependencies and only reviewed at
their Aurora integration seams.

**Reviewer posture:** This is an experimental engine, not a production browser
(per `README.md`). The findings below are calibrated to Aurora's *own* stated
security model — a capability-gated engine — and flag where that model does not
hold. Severities use CVSS-style qualitative bands (Critical/High/Medium/Low).

> ⚠️ The headline result: **the capability model is not enforced on the path that
> matters most.** Page JavaScript reaches the network directly, below the
> capability gate. A `Capabilities::sandboxed()` browser still makes arbitrary
> outbound requests.

---

## Summary table

| ID | Severity | Title | CWE | Primary location |
|----|----------|-------|-----|------------------|
| AUR-001 | **Critical** | JS `fetch`/XHR bypass the capability model | CWE-862 | `src/js_v8/runtime.rs:79,1431` |
| AUR-002 | **High** | Server-Side Request Forgery (no internal-address filtering) | CWE-918 | `src/fetch/http.rs` |
| AUR-003 | **High** | `file://` read is not confined to a workspace (path traversal / full-FS read) | CWE-22 / CWE-552 | `src/fetch/api.rs:17-20,31-34` |
| AUR-004 | **Medium** | No same-origin policy / CORS; cross-origin response bodies readable | CWE-346 | `src/js_v8/runtime.rs`, `src/fetch/http.rs:202` |
| AUR-005 | **Medium** | Unbounded `NodeData` leak + unbounded response size → memory-exhaustion DoS | CWE-401 / CWE-770 | `src/js_v8/node_create.rs:47`, `src/fetch/http.rs:189` |
| AUR-006 | **Medium** | Latent UB in V8-External / Stylo `unsafe` seams | CWE-416 / CWE-787 | `src/js_v8/**`, `src/stylo_bridge/arena.rs` |
| AUR-007 | **Low** | Predictable temp file for media decode (symlink / TOCTOU) | CWE-377 | `src/media.rs:185-190` |
| AUR-008 | **Low** | Container image runs as root | CWE-250 | `Dockerfile` |
| AUR-009 | **Info** | Dependency hygiene: pinned alphas, forked Stylo, no `cargo audit` in CI | CWE-1104 | `Cargo.toml` / `Cargo.lock` |

---

## AUR-001 — JavaScript network bypasses the capability gate (Critical)

**CWE-862: Missing Authorization.**

Aurora's differentiator is capability gating: `fetch/api.rs` checks
`require_network_access` / `require_file_access` before any I/O, and
`Capabilities::sandboxed()` is documented as "a browser that cannot reach the
network." That check lives **only** in `src/fetch/api.rs`.

The V8 runtime does not use `fetch/api.rs`. The JS-facing `fetch`/`XMLHttpRequest`
shims call the transport layer directly, with no `Identity` and no capability
check:

```rust
// src/js_v8/runtime.rs:1430  (aurora_fetch_sync)
let result =
    crate::fetch::http::fetch_response_with_method(&url, &method, body_arg, &request_headers)
        .map_err(|error| error.to_string());

// src/js_v8/runtime.rs:79  (async NetworkTasks worker — same direct call)
let result = crate::fetch::http::fetch_response_with_method(...);
```

`fetch_response_with_method` (and the whole `src/fetch/http.rs` module) is `pub`
and takes no identity. `aurora_fetch_sync` / `aurora_fetch_start` /
`aurora_fetch_poll` (`runtime.rs:1406,1449,1469`) read the URL straight from JS
arguments and forward it.

**Impact.** Any script in a loaded page — including a fully `sandboxed()` embed —
can issue arbitrary `GET`/`POST`/etc. to any reachable host. The capability model
is advisory for the one actor (untrusted page JS) it most needs to constrain. The
Rust-side `fetch::fetch_*` gate only covers the engine's *own* document/subresource
fetches, not script-initiated traffic.

**Remediation.**
- Thread the active `Identity` into the JS runtime and call
  `require_network_access` before every script-initiated request; route JS network
  through the same gated `fetch::api` entry points rather than `fetch::http`.
- Make `fetch::http` non-`pub` (or `pub(crate)` behind an identity-bearing wrapper)
  so the gate cannot be skipped by construction.
- Add a regression test: a `sandboxed()` browser running
  `fetch('http://example.com')` must fail.

---

## AUR-002 — SSRF: no filtering of internal / metadata addresses (High)

**CWE-918: Server-Side Request Forgery.**

`src/fetch/http.rs` builds a reqwest client and sends the URL verbatim. There is:
- no allow/deny list for destination hosts,
- no rejection of loopback (`127.0.0.0/8`, `::1`), link-local
  (`169.254.0.0/16` — incl. the `169.254.169.254` cloud-metadata endpoint),
  private (RFC 1918), or `localhost`,
- and reqwest's **default redirect policy is left in place** (`Client::builder()`
  at `http.rs:49` sets no `.redirect(...)`), so up to 10 redirects are followed
  automatically. A public URL can `302` to an internal target, defeating any
  naïve host check applied only to the initial URL.

Combined with **AUR-001**, the request URL is attacker-controlled from page JS.

**Impact.** Page JavaScript (or a malicious document) can probe and read internal
services, hit cloud metadata endpoints to steal credentials, and pivot through the
host's network position.

**Remediation.**
- Resolve the destination and reject non-public IP ranges *before* connecting, and
  re-validate on every redirect hop (set an explicit `redirect::Policy` with a
  custom check, or disable auto-redirects and validate each hop yourself).
- Gate "may reach private networks" behind an explicit capability separate from
  general `NetworkAccess`.

---

## AUR-003 — `file://` access is not confined to a workspace (High)

**CWE-22 (Path Traversal) / CWE-552 (Files Accessible to External Parties).**

The capability is named `ReadWorkspace`, but the implementation reads any absolute
path with no workspace root and no containment:

```rust
// src/fetch/api.rs
if let Some(path) = url.strip_prefix("file://") {
    require_file_access(identity)?;
    return std::fs::read_to_string(path).map_err(FetchError::Io);   // any path
}
```

`require_file_access` (`fetch/capability.rs:5`) only checks that the bit is set; it
never compares `path` against a base directory. Relative resolution
(`fetch/resolve.rs:82` `normalize_path`) collapses `..` components purely
lexically, so `file://../../etc/passwd` style references resolve outside any
intended root, and `file:///etc/passwd` is read directly.

**Impact.** An identity granted `ReadWorkspace` (the CLI sets this with
`--allow-workspace-read`, `main.rs:43`) can read the entire filesystem the process
can access, not just a workspace. If subresource URLs in a document are
attacker-influenced, this becomes arbitrary local file disclosure. Note JS `fetch`
currently only handles http(s), which limits the *script* reach today — but the
Rust document/subresource path is exposed.

**Remediation.**
- Introduce a concrete workspace root; canonicalize the requested path
  (`std::fs::canonicalize`) and verify it is a prefix-descendant of the root *after*
  symlink resolution (guard against symlink escape too).
- Reject absolute `file://` paths that escape the root; do not rely on lexical
  `..` stripping.

---

## AUR-004 — No same-origin policy / CORS; header allow-list is permissive (Medium)

**CWE-346: Origin Validation Error.**

`fetch_response_with_method` returns the full body and headers of any response to
the calling script (`runtime.rs:set_fetch_result`). There is no origin check, no
CORS preflight, and no opaque-response model — script can read cross-origin
response bodies directly. The request-header filter
(`is_forbidden_request_header`, `http.rs:202`) blocks `cookie`/`origin`/`referer`
but **explicitly allows `authorization`** (asserted in the test at `http.rs:250`)
and any non-listed custom header.

**Impact.** A page can read responses from arbitrary third-party origins and attach
credentials of its choosing — a data-exfiltration / cross-origin read primitive a
real browser's SOP would prevent. Less severe here because Aurora has no cookie jar
or ambient session today, but it removes a load-bearing browser invariant and
compounds AUR-001/002.

**Remediation.** Track each document's origin; enforce SOP for response-body
readability and a CORS model for cross-origin requests; reconsider letting scripts
set `authorization` for cross-origin destinations.

---

## AUR-005 — Memory-exhaustion DoS: leaked `NodeData` + unbounded response bodies (Medium)

**CWE-401 (Missing Release of Memory) / CWE-770 (Allocation Without Limits).**

1. **Leaked DOM wrappers.** Every JS node wrapper allocates and *leaks* a
   `NodeData`:

   ```rust
   // src/js_v8/node_create.rs:47
   let node_data = Box::into_raw(Box::new(NodeData { node, registry, document, .. })) as *mut _;
   let node_external = v8::External::new(scope, node_data);
   ```

   There is no `Box::from_raw`, no V8 weak callback, and no finalizer anywhere in
   `src/js_v8/` (confirmed: no `from_raw` / `SetWeak` / `MakeWeak`). Each leaked
   `NodeData` also pins a `NodePtr` clone **and** an `Rc<NodeRegistry>` **and** the
   document `NodePtr`, so it transitively retains DOM state. The leak is what keeps
   the raw-pointer deref sound (see AUR-006), but it means wrapper memory grows
   monotonically with DOM churn. A page that creates/discards many elements
   (trivial in JS) drives unbounded growth.

2. **Unbounded response reads.** `http.rs:189` does `response.bytes()` with no
   maximum size; a large or slow `Content-Length`-lying response is fully buffered
   in memory.

**Impact.** A hostile or merely heavy page can exhaust process memory (engine DoS),
especially in a long-running agent-controlled session.

**Remediation.** Reclaim `NodeData` via a V8 weak/finalizer callback (or store it in
a registry-owned slab keyed by node id and free on node removal). Impose a
configurable max response size and stream/early-abort beyond it.

---

## AUR-006 — Latent undefined behavior in `unsafe` integration seams (Medium)

**CWE-416 (Use-After-Free) / CWE-787 (Out-of-bounds Write) — latent.**

~124 `unsafe` uses across `src/js_v8/**` and `src/stylo_bridge/**`. Two patterns
carry the most soundness risk:

- **V8 External raw deref.** Dozens of sites do
  `unsafe { &*(external.value() as *const NodeData) }`
  (e.g. `node_create.rs:436,669,…`, `style_class/classlist.rs:25,…`,
  `runtime.rs:1464,1476`). Soundness depends entirely on the pointer's target
  outliving every JS reference. Today that holds *only because the `Box` is
  deliberately leaked* (AUR-005). The moment anyone adds reclamation, or a stale
  External survives an isolate teardown/reuse, these become use-after-free. This
  is a fragile, undocumented invariant.

- **`unsafe impl Send + Sync` over `UnsafeCell`.** `StyloNodeData`
  (`stylo_bridge/arena.rs:35-36`) and `ArenaNode` (`arena.rs:98-99`) assert
  `Send + Sync` around an `UnsafeCell<Option<ElementDataWrapper>>` with interior
  mutation through `unsafe fn ensure_init`/`clear`/`get_mut`. The safety comment
  ("sequential traversal only — stylo holds exclusive logical access") is the *only*
  thing preventing a data race; Stylo's parallel traversal feature or any future
  concurrent caller would violate it with no compile-time guard.

**Impact.** No proven exploit in the current single-threaded, leak-everything
configuration, but the invariants are implicit and easy to break during the active
blitz-dom migration noted in project memory.

**Remediation.** Document each `// SAFETY:` invariant precisely; prefer a typed
slab/arena with generational indices over raw `External` pointers; gate the
`Send/Sync` asserts behind a sequential-traversal type-state or replace with a real
lock if parallel styling is ever enabled. Run `cargo +nightly miri` over the
js_v8/stylo_bridge unit tests.

---

## AUR-007 — Predictable temp file for media decode (Low)

**CWE-377: Insecure Temporary File.**

```rust
// src/media.rs:187
let path = std::env::temp_dir().join(format!("aurora-media-{}-{id}.bin", std::process::id()));
std::fs::write(&path, bytes)?;        // follows symlinks; default perms
```

The filename is fully predictable (`pid` + a monotonic counter) and lives in the
world-writable temp dir. `fs::write` follows an existing symlink at that path and
creates with default permissions. On a shared host an attacker can pre-create the
path as a symlink (write redirection) or race to read decoded media. (`media-ffmpeg`
is off by default, limiting exposure.)

**Remediation.** Use `tempfile::NamedTempFile` (atomic `O_EXCL` create, 0600,
auto-cleanup) instead of a predictable name + `fs::write`.

---

## AUR-008 — Container runs as root (Low)

**CWE-250: Execution with Unnecessary Privileges.**

`Dockerfile` runtime stage defines no `USER`; the entrypoint runs as root. Combined
with AUR-002/003, a successful in-process compromise gets root inside the container.

**Remediation.** Add a non-root user and `USER` directive in the runtime stage;
drop capabilities; run read-only rootfs where feasible.

---

## AUR-009 — Dependency hygiene (Info)

**CWE-1104: Use of Unmaintained/Unvetted Components.**

- blitz-dom / blitz-html / blitz-paint / blitz-traits pinned to `0.3.0-alpha.5`
  (pre-release), `anyrender` 0.11, `v8` 150 — all fast-moving.
- Stylo is a **local fork** (`third_party/stylo`, `[patch.crates-io]`) carrying a
  one-line divergence from 0.18.0; forks drift out of upstream security fixes.
- No `cargo audit` / `cargo deny` step is present in the repo.

Observed lockfile versions (`Cargo.lock`) are current at review time (e.g.
`rustls 0.23.37`, `tokio 1.52.3`, `hyper 1.9.0`, `chrono 0.4.45`) with no obvious
known-vulnerable pin, but this was a manual spot-check.

**Remediation.** Add `cargo audit` (RustSec advisory DB) and `cargo deny` to CI;
track upstream Stylo security releases against the fork; schedule alpha-dependency
bumps.

---

## Recommended remediation order

1. **AUR-001** — gate JS network through the capability model (the model is
   currently bypassable, which undermines the project's core claim).
2. **AUR-002 / AUR-003** — SSRF filtering + redirect re-validation; confine
   `file://` to a canonicalized workspace root.
3. **AUR-005** — reclaim leaked `NodeData` and cap response size.
4. **AUR-004 / AUR-006** — origin model; document and harden the `unsafe` seams
   (Miri), before the blitz-dom migration changes object lifetimes.
5. **AUR-007 / AUR-008 / AUR-009** — temp-file, container, and supply-chain hygiene.

---

## Verification notes

- Findings are static-analysis based against the `security/complete-cve-analysis`
  branch; file:line references are from that tree.
- No `cargo audit` was run (no network in this environment); AUR-009 lists the
  lockfile spot-check actually performed.
- AUR-001 is directly testable and recommended as the first regression test:
  a `Capabilities::sandboxed()` browser must not be able to complete a scripted
  `fetch()`.

*Generated as part of a defensive security review of Aurora.*
