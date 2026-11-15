#!/usr/bin/env python3
"""Run imported web-platform-tests through Aurora and compare with expectations.

    scripts/wpt.py fetch                    # sparse checkout of the pinned WPT commit
    scripts/wpt.py run [--update] [path...] # run tests, print pass counts

Tests are served from the checkout ($AURORA_WPT, default .wpt/) over a local
HTTP server, so absolute paths like /resources/testharness.js resolve. The
server also generates the wrapper pages wptserve builds for .any.js and
.window.js tests, and serves scripts/wpt/testharnessreport.js in place of
WPT's own. Each test runs in a fresh headless `aurora` process; the
AURORA_PROBE_JS hook prints the collected results when the page settles.

tests/wpt/expectations.json records, per test, the harness status and the
subtests that pass. Entries marked "flaky": true (timing-sensitive tests) are
run and counted but never compared; --update keeps the mark. `run` exits 1 when a passing subtest stops passing or a
test's harness status gets worse. Newly passing subtests are reported;
--update rewrites the expectations to the current results.
"""

import argparse
import concurrent.futures
import http.server
import json
import os
import re
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WPT_REPO = "https://github.com/web-platform-tests/wpt.git"
WPT_COMMIT = "c0f2b11f490d5849bf4d48c276b68681e894b1fb"
# Imported test directories, plus the shared helpers they load.
TEST_DIRS = [
    "dom/nodes",
    "dom/events",
    "custom-elements",
    "shadow-dom",
    "html/webappapis/timers",
]
SUPPORT_DIRS = ["resources", "common"]
EXPECTATIONS = ROOT / "tests" / "wpt" / "expectations.json"
REPORTER = ROOT / "scripts" / "wpt" / "testharnessreport.js"
DUMP = ROOT / "scripts" / "wpt" / "dump.js"

TEST_STATUS = ["PASS", "FAIL", "TIMEOUT", "NOTRUN", "PRECONDITION_FAILED"]
HARNESS_STATUS = ["OK", "ERROR", "TIMEOUT", "PRECONDITION_FAILED"]
# Harness outcomes from best to worst; a move right is a regression.
HARNESS_RANK = ["OK", "PRECONDITION_FAILED", "ERROR", "TIMEOUT", "CRASH"]

META = re.compile(r"^//\s*META:\s*(\w+)=(.*)$")
SKIP_PARTS = {"resources", "support", "crashtests", "tools"}


def wpt_root() -> Path:
    return Path(os.environ.get("AURORA_WPT", ROOT / ".wpt"))


# --- fetch -----------------------------------------------------------------

def fetch(root: Path) -> None:
    def git(*args):
        subprocess.run(["git", "-C", str(root), *args], check=True)

    if not (root / ".git").exists():
        root.mkdir(parents=True, exist_ok=True)
        git("init", "-q")
        git("remote", "add", "origin", WPT_REPO)
    git("sparse-checkout", "set", "--cone", *SUPPORT_DIRS, *TEST_DIRS)
    git("fetch", "-q", "--depth", "1", "--filter=blob:none", "origin", WPT_COMMIT)
    git("checkout", "-q", "--detach", WPT_COMMIT)
    print(f"WPT {WPT_COMMIT[:12]} checked out in {root}")


# --- discovery -------------------------------------------------------------

def parse_meta(source: str) -> dict:
    """Collect `// META: key=value` lines from the head of a .any.js file."""
    meta = {}
    for line in source.splitlines():
        match = META.match(line.strip())
        if not match:
            if line.strip() and not line.startswith("//"):
                break
            continue
        meta.setdefault(match.group(1), []).append(match.group(2).strip())
    return meta


def runs_in_window(meta: dict) -> bool:
    globals_ = ",".join(meta.get("global", ["window,dedicatedworker"]))
    scopes = {g.strip() for g in globals_.split(",")}
    return bool(scopes & {"window", "default"})


def discover(root: Path, filters: list) -> list:
    """Test URLs (paths relative to the WPT root) in TEST_DIRS."""
    found = []
    for test_dir in TEST_DIRS:
        for path in sorted((root / test_dir).rglob("*")):
            rel = path.relative_to(root).as_posix()
            if not path.is_file() or SKIP_PARTS & set(rel.split("/")[:-1]):
                continue
            name = path.name
            if "-manual" in name or name.endswith("-ref.html"):
                continue
            if name.endswith(".any.js") or name.endswith(".window.js"):
                source = path.read_text(errors="replace")
                if name.endswith(".any.js") and not runs_in_window(parse_meta(source)):
                    continue
                found.append(rel[: -len(".js")] + ".html")
            elif name.endswith(".html") and "/resources/testharness.js" in path.read_text(errors="replace"):
                found.append(rel)
    if filters:
        found = [t for t in found if any(t.startswith(f.rstrip("/")) for f in filters)]
    return found


# --- server ----------------------------------------------------------------

def wrapper_page(script_path: str, meta: dict) -> str:
    """The page wptserve generates for a .any.js / .window.js test."""
    lines = ["<!doctype html>", "<meta charset=utf-8>"]
    for title in meta.get("title", [])[:1]:
        lines.append(f"<title>{title}</title>")
    lines.append(
        "<script>self.GLOBAL = {isWindow: function() { return true; },"
        " isWorker: function() { return false; },"
        " isShadowRealm: function() { return false; }};</script>"
    )
    lines.append('<script src="/resources/testharness.js"></script>')
    lines.append('<script src="/resources/testharnessreport.js"></script>')
    for src in meta.get("script", []):
        lines.append(f'<script src="{src}"></script>')
    lines.append("<div id=log></div>")
    lines.append(f'<script src="/{script_path}"></script>')
    return "\n".join(lines) + "\n"


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def send_text(self, body: str, content_type: str):
        data = body.encode()
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        path = self.path.split("?", 1)[0].split("#", 1)[0]
        if path == "/resources/testharnessreport.js":
            return self.send_text(REPORTER.read_text(), "text/javascript")
        for suffix in (".any.html", ".window.html"):
            if path.endswith(suffix):
                script = path[1 : -len(".html")] + ".js"
                source = Path(self.directory) / script
                if source.is_file():
                    meta = parse_meta(source.read_text(errors="replace"))
                    return self.send_text(wrapper_page(script, meta), "text/html; charset=utf-8")
        return super().do_GET()


def start_server(root: Path) -> http.server.ThreadingHTTPServer:
    handler = lambda *a, **kw: Handler(*a, directory=str(root), **kw)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


# --- running ---------------------------------------------------------------

def parse_report(output: str) -> dict:
    """Turn a run's output into {"harness": status, "subtests": {name: status}}."""
    report = None
    for line in output.splitlines():
        if line.startswith("[wpt] "):
            try:
                report = json.loads(line[len("[wpt] ") :])
            except ValueError:
                pass
    if not isinstance(report, dict) or report.get("missing"):
        return {"harness": "CRASH", "subtests": {}}
    subtests = {}
    for test in report.get("tests", []):
        status = test.get("status")
        subtests[str(test.get("name"))] = (
            TEST_STATUS[status] if isinstance(status, int) and 0 <= status < len(TEST_STATUS) else "FAIL"
        )
    harness = "TIMEOUT"
    if report.get("complete"):
        status = (report.get("harness") or {}).get("status")
        harness = HARNESS_STATUS[status] if isinstance(status, int) and 0 <= status < len(HARNESS_STATUS) else "ERROR"
    return {"harness": harness, "subtests": subtests}


def run_test(binary: Path, base_url: str, test: str, timeout: int) -> dict:
    # Aurora writes logs/ into its working directory; keep that out of the repo.
    with tempfile.TemporaryDirectory() as cwd:
        env = dict(os.environ, AURORA_HEADLESS="1", AURORA_PROBE_JS=str(DUMP))
        for key in ("AURORA_SCREENSHOT", "AURORA_REPLAY", "AURORA_RECORD"):
            env.pop(key, None)
        try:
            proc = subprocess.run(
                [str(binary), f"{base_url}/{test}"],
                env=env,
                cwd=cwd,
                capture_output=True,
                text=True,
                errors="replace",
                timeout=timeout,
            )
            output = proc.stdout + proc.stderr
        except subprocess.TimeoutExpired:
            return {"harness": "CRASH", "subtests": {}}
    return parse_report(output)


# --- expectations ----------------------------------------------------------

def to_expectations(results: dict) -> dict:
    return {
        test: {
            "harness": result["harness"],
            "pass": sorted(n for n, s in result["subtests"].items() if s == "PASS"),
            "subtests": len(result["subtests"]),
        }
        for test, result in sorted(results.items())
    }


def dump_expectations(expected: dict) -> str:
    """One test per line, so a change to one test is a one-line diff."""
    lines = [
        f"{json.dumps(test)}: {json.dumps(entry, sort_keys=True, ensure_ascii=False)}"
        for test, entry in sorted(expected.items())
    ]
    return "{\n" + ",\n".join(lines) + "\n}\n"


def compare(results: dict, expected: dict) -> tuple:
    """(regressions, improvements) as human-readable lines."""
    worse, better = [], []
    for test, result in sorted(results.items()):
        old = expected.get(test)
        if old is not None and old.get("flaky"):
            continue
        if old is None:
            better.append(f"{test}: new test, {count_passes(result)} passing")
            continue
        old_rank = HARNESS_RANK.index(old["harness"]) if old["harness"] in HARNESS_RANK else 0
        new_rank = HARNESS_RANK.index(result["harness"])
        if new_rank > old_rank:
            worse.append(f"{test}: harness {old['harness']} -> {result['harness']}")
        elif new_rank < old_rank:
            better.append(f"{test}: harness {old['harness']} -> {result['harness']}")
        now_passing = {n for n, s in result["subtests"].items() if s == "PASS"}
        for name in sorted(set(old["pass"]) - now_passing):
            worse.append(f"{test}: {name!r} {result['subtests'].get(name, 'missing')}")
        for name in sorted(now_passing - set(old["pass"])):
            better.append(f"{test}: {name!r} now passes")
    return worse, better


def count_passes(result: dict) -> int:
    return sum(1 for s in result["subtests"].values() if s == "PASS")


def summary(results: dict) -> str:
    rows = [["directory", "files", "harness ok", "subtests", "pass", "%"]]
    totals = [0, 0, 0, 0]
    for test_dir in TEST_DIRS:
        group = [r for t, r in results.items() if t.startswith(test_dir + "/")]
        if not group:
            continue
        counts = [
            len(group),
            sum(1 for r in group if r["harness"] == "OK"),
            sum(len(r["subtests"]) for r in group),
            sum(count_passes(r) for r in group),
        ]
        totals = [a + b for a, b in zip(totals, counts)]
        rows.append([test_dir, *map(str, counts), percent(counts[3], counts[2])])
    rows.append(["total", *map(str, totals), percent(totals[3], totals[2])])
    widths = [max(len(row[i]) for row in rows) for i in range(len(rows[0]))]
    return "\n".join(
        "  ".join(c.rjust(w) if i else c.ljust(w) for i, (c, w) in enumerate(zip(row, widths)))
        for row in rows
    )


def percent(part: int, whole: int) -> str:
    return f"{100 * part / whole:.1f}" if whole else "-"


def run(args) -> int:
    root = wpt_root()
    if not (root / "resources" / "testharness.js").exists():
        sys.exit(f"no WPT checkout in {root}; run scripts/wpt.py fetch")
    binary = Path(os.environ.get("AURORA_BIN", ROOT / "target" / "release" / "aurora"))
    if not binary.exists():
        sys.exit(f"{binary} not found; run cargo build --release")
    tests = discover(root, args.paths)
    if not tests:
        sys.exit("no tests matched")

    server = start_server(root)
    base_url = f"http://127.0.0.1:{server.server_address[1]}"
    print(f"running {len(tests)} tests with {args.jobs} jobs...", file=sys.stderr)
    started = time.monotonic()
    results = {}
    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        futures = {pool.submit(run_test, binary, base_url, t, args.timeout): t for t in tests}
        for done, future in enumerate(concurrent.futures.as_completed(futures), 1):
            results[futures[future]] = future.result()
            if args.verbose:
                r = results[futures[future]]
                print(f"[{done}/{len(tests)}] {futures[future]} {r['harness']} "
                      f"{count_passes(r)}/{len(r['subtests'])}", file=sys.stderr)
    server.shutdown()
    print(f"done in {time.monotonic() - started:.0f}s", file=sys.stderr)

    print(summary(results))
    if args.json:
        args.json.write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")

    expected = json.loads(EXPECTATIONS.read_text()) if EXPECTATIONS.exists() else {}
    if args.paths:
        expected = {t: e for t, e in expected.items() if t in results}
    if args.update:
        merged = dict(json.loads(EXPECTATIONS.read_text()) if EXPECTATIONS.exists() else {})
        for test, entry in to_expectations(results).items():
            if merged.get(test, {}).get("flaky"):
                entry["flaky"] = True
            merged[test] = entry
        EXPECTATIONS.parent.mkdir(parents=True, exist_ok=True)
        EXPECTATIONS.write_text(dump_expectations(merged))
        print(f"updated {EXPECTATIONS.relative_to(ROOT)}")
        return 0

    missing = [f"{t}: expected test was not run" for t in expected if t not in results]
    worse, better = compare(results, expected)
    for line in better:
        print(f"IMPROVED {line}")
    for line in missing + worse:
        print(f"REGRESSION {line}")
    if better:
        print("run with --update to record improvements")
    return 1 if missing or worse else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("fetch", help="check out the pinned WPT commit")
    run_parser = sub.add_parser("run", help="run tests and compare with expectations")
    run_parser.add_argument("paths", nargs="*", help="only tests under these paths")
    run_parser.add_argument("--update", action="store_true", help="rewrite expectations")
    run_parser.add_argument("--json", type=Path, help="write full results as JSON")
    run_parser.add_argument("--jobs", type=int, default=os.cpu_count() or 2)
    run_parser.add_argument("--timeout", type=int, default=60, help="seconds per test")
    run_parser.add_argument("-v", "--verbose", action="store_true", help="print each test")
    args = parser.parse_args()
    if args.command == "fetch":
        fetch(wpt_root())
        return 0
    if args.jobs <= 0 or args.timeout <= 0:
        parser.error("--jobs and --timeout must be positive")
    return run(args)


if __name__ == "__main__":
    sys.exit(main())
