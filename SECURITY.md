# Security Policy

Aurora is an experimental browser engine. It is **not hardened for hostile
content** and should not be used to browse untrusted sites or to process
untrusted HTML/JavaScript where a compromise would matter.

This document describes what is in scope, what isn't, and how to report an issue.

## Supported Versions

Aurora is pre-1.0 and has no maintained release branches. Only the latest
commit on `main` (and the latest published `aurora-engine` crate version) is
supported. Fixes land on `main`; there are no backports.

| Version            | Supported |
| ------------------ | --------- |
| `main`             | ✅        |
| latest `0.x` crate | ✅        |
| anything older     | ❌        |

## Reporting a Vulnerability

**Please do not open a public issue for security problems.**

Report privately through GitHub's private vulnerability reporting:
<https://github.com/JohannaWeb/Aurora/security/advisories/new>

Include, where possible:

- the Aurora commit or crate version;
- how Aurora was used (the `aurora` binary, headless screenshot, or the
  embedding API) and which `Capabilities` were granted;
- a minimal HTML/JS reproducer or URL;
- what happened and what you expected (crash, capability bypass, file read,
  memory corruption, etc.).

Aurora is maintained by one person, so response times are best-effort.
Expect an acknowledgement within about a week. Fixed issues are disclosed in a
GitHub Security Advisory once a fix is on `main`. Credit is given unless you
ask otherwise.

## Security Model

Aurora's security boundary is its **capability model**. An identity or
`aurora::Capabilities` grant decides which ambient powers a page gets:

| Capability      | Gates                                                  |
| --------------- | ------------------------------------------------------ |
| `NetworkAccess` | outbound `http://` / `https://` fetches                |
| `ReadWorkspace` | `file://` reads from the local filesystem              |

`Capabilities::sandboxed()` grants neither. **Note:** `Capabilities::default()`
is permissive (`all()`), matching the standalone browser. Embedders that load
untrusted content should opt into `sandboxed()` explicitly and grant only what
they need.

### In scope

We especially want reports of:

- **Capability bypasses**: a page reaching the network or the local
  filesystem without the corresponding capability, e.g. through subresource
  loads, redirects, `data:`/`file:` URL tricks, script-initiated fetch/XHR, or
  any code path that skips `require_network_access` /
  `require_file_access`.
- **Memory safety bugs in Aurora-owned code**: `unsafe` blocks, the V8 ↔ DOM
  bridge (`src/js_v8/`), handle/pointer lifetime errors, use-after-free
  reachable from page script.
- **Path traversal or arbitrary file access** through `file://` handling,
  fixtures, screenshot output paths, or CLI arguments derived from page
  content.
- **Crashes or panics reachable from page content** that could plausibly
  be escalated beyond a denial of service.
- **Bugs in Aurora's patches to the vendored crates** under `third_party/`
  (`blitz-dom`, `blitz-paint`); see their `PATCHES.md` files.

### Out of scope / known limitations

These are known gaps, not vulnerabilities. Please don't report them unless you
have found something that goes beyond them:

- **No process isolation or OS sandbox.** V8, layout, and paint all run in a
  single process with the user's privileges. There is no site isolation.
- **No web origin security model.** Same-origin policy, CORS, CSP, mixed-content
  blocking, and cookie isolation are not implemented (or only stubbed).
  Within the granted capabilities, any page can fetch any URL.
- **Partial/stubbed Web APIs.** Many DOM/BOM APIs are compatibility shims that
  may behave incorrectly. Spec deviations are bugs, not security issues, unless
  they break the capability boundary.
- **Denial of service** from heavy pages, infinite loops, or resource
  exhaustion (CPU, memory, GPU).
- **Vulnerabilities in upstream dependencies** (V8, Stylo, Vello, wgpu,
  reqwest, rustls, etc.). Please report those upstream. Do tell us if Aurora
  pins an affected version or uses a dependency in an unsafe way.

## Hardening Roadmap

Planned work toward a stronger boundary (goals, not current guarantees):
finer-grained capabilities (JS execution, storage), per-origin policy,
DID-based identity for capability grants, and process-level sandboxing of the
content engine.
