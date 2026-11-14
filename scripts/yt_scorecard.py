#!/usr/bin/env python3
"""Replay every capture and print a scorecard.

    scripts/yt_scorecard.py [--json out.json] [--baseline old.json] [name...]

Each capture in $AURORA_CAPTURES (default .captures/, recorded with
scripts/yt_capture.sh) is loaded with AURORA_REPLAY, so runs are offline and
reproducible. scripts/scorecard/probe.js measures the composed tree after
boot; this script adds replay misses, JS errors and wall time.

With --baseline, each metric is compared with an earlier --json output, and
the exit status is 1 if any page got worse.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROBE = ROOT / "scripts" / "scorecard" / "probe.js"

# Metric -> +1 if higher is better, -1 if lower is better. Columns in order.
METRICS = {
    "elements": +1,
    "text_chars": +1,
    "items": +1,
    "distinct_videos": +1,
    "duplicated_videos": -1,
    "misplaced_items": -1,
    "height": +1,
    "js_errors": -1,
    "replay_misses": -1,
}
SHORT = {
    "elements": "elems",
    "text_chars": "text",
    "items": "items",
    "distinct_videos": "videos",
    "duplicated_videos": "dup",
    "misplaced_items": "misplc",
    "height": "height",
    "js_errors": "jserr",
    "replay_misses": "miss",
}

JS_ERROR = re.compile(r"Uncaught |\[yt-life\] ERROR ")


def capture_url(capture: Path) -> str:
    meta = capture / "capture.json"
    if meta.exists():
        return json.loads(meta.read_text())["url"]
    with open(capture / "index.jsonl") as index:
        return json.loads(index.readline())["url"]


def run_capture(binary: Path, capture: Path, timeout: int) -> dict:
    url = capture_url(capture)
    with tempfile.TemporaryDirectory() as tmp:
        env = dict(
            os.environ,
            AURORA_REPLAY=str(capture),
            AURORA_PROBE_JS=str(PROBE),
            AURORA_SCREENSHOT=str(Path(tmp) / "page.png"),
        )
        env.pop("AURORA_RECORD", None)
        started = time.monotonic()
        try:
            proc = subprocess.run(
                [str(binary), url],
                env=env,
                cwd=ROOT,
                capture_output=True,
                text=True,
                errors="replace",
                timeout=timeout,
            )
            output, status = proc.stdout + proc.stderr, proc.returncode
        except subprocess.TimeoutExpired as expired:
            output = (expired.stdout or b"").decode(errors="replace") + (
                expired.stderr or b""
            ).decode(errors="replace")
            status = "timeout"
        seconds = time.monotonic() - started

    result = {"url": url, "status": status, "seconds": round(seconds, 1)}
    for line in output.splitlines():
        if line.startswith("[scorecard] "):
            result.update(json.loads(line[len("[scorecard] ") :]))
    result["js_errors"] = sum(1 for line in output.splitlines() if JS_ERROR.search(line))
    result["replay_misses"] = output.count("replay miss:")
    if "elements" not in result:
        result["error"] = "probe did not report"
    return result


def table(results: dict, baseline: dict) -> str:
    header = ["page"] + [SHORT[m] for m in METRICS] + ["secs"]
    rows = [header]
    for name, result in results.items():
        row = [name]
        for metric in METRICS:
            value = result.get(metric, "-")
            old = baseline.get(name, {}).get(metric)
            if isinstance(value, int) and isinstance(old, int) and value != old:
                value = f"{value}({value - old:+d})"
            row.append(str(value))
        row.append(str(result.get("seconds", "-")))
        rows.append(row)
    widths = [max(len(row[i]) for row in rows) for i in range(len(header))]
    return "\n".join(
        "  ".join(cell.rjust(width) if i else cell.ljust(width) for i, (cell, width) in enumerate(zip(row, widths)))
        for row in rows
    )


def regressions(results: dict, baseline: dict) -> list:
    found = []
    for name, result in results.items():
        old = baseline.get(name)
        if old is None:
            continue
        if "error" in result and "error" not in old:
            found.append(f"{name}: {result['error']}")
        for metric, direction in METRICS.items():
            new_value, old_value = result.get(metric), old.get(metric)
            if isinstance(new_value, int) and isinstance(old_value, int):
                if (new_value - old_value) * direction < 0:
                    found.append(f"{name}: {metric} {old_value} -> {new_value}")
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("names", nargs="*", help="captures to run (default: all)")
    parser.add_argument("--json", type=Path, help="write results as JSON")
    parser.add_argument("--baseline", type=Path, help="compare with an earlier --json file")
    parser.add_argument("--timeout", type=int, default=300, help="seconds per page")
    args = parser.parse_args()

    captures = Path(os.environ.get("AURORA_CAPTURES", ROOT / ".captures"))
    binary = Path(os.environ.get("AURORA_BIN", ROOT / "target" / "release" / "aurora"))
    if not binary.exists():
        sys.exit(f"{binary} not found; run cargo build --release")
    pages = sorted(p for p in captures.glob("*") if (p / "index.jsonl").exists())
    if args.names:
        pages = [p for p in pages if p.name in args.names]
    if not pages:
        sys.exit(f"no captures in {captures}; record them with scripts/yt_capture.sh")

    results = {}
    for page in pages:
        print(f"replaying {page.name}...", file=sys.stderr)
        results[page.name] = run_capture(binary, page, args.timeout)

    baseline = json.loads(args.baseline.read_text()) if args.baseline else {}
    print(table(results, baseline))
    if args.json:
        args.json.write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")

    worse = regressions(results, baseline)
    for line in worse:
        print(f"REGRESSION {line}")
    return 1 if worse else 0


if __name__ == "__main__":
    sys.exit(main())
