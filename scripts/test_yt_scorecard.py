"""Regression checks for the scorecard's exit status and report handling."""

import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import yt_scorecard as scorecard


def valid_result(**changes):
    result = dict.fromkeys(scorecard.METRICS, 0)
    result.update(status=0, elements=100, height=800)
    result.update(changes)
    return result


class ScorecardTests(unittest.TestCase):
    def parse_run(self, output="", status=0, error=None):
        process = subprocess.CompletedProcess([], status, output, "")
        with (
            patch.object(scorecard, "capture_url", return_value="https://example.test/"),
            patch.object(scorecard.subprocess, "run", return_value=process, side_effect=error),
        ):
            return scorecard.run_capture(Path("aurora"), Path("capture"), 1)

    def test_successful_complete_probe(self):
        result = self.parse_run("[scorecard] " + json.dumps(valid_result()))
        self.assertEqual(scorecard.failures({"home": result}), [])

    def test_crash_after_probe_cannot_be_hidden_by_probe_status(self):
        result = self.parse_run("[scorecard] " + json.dumps(valid_result()), status=-11)
        self.assertEqual(result["status"], -11)
        self.assertTrue(scorecard.failures({"home": result}))

    def test_timeout_retains_partial_probe_but_fails(self):
        output = ("[scorecard] " + json.dumps(valid_result())).encode()
        result = self.parse_run(error=subprocess.TimeoutExpired("aurora", 1, output=output))
        self.assertEqual(result["elements"], 100)
        self.assertEqual(result["status"], "timeout")
        self.assertTrue(scorecard.failures({"home": result}))

    def test_launch_failure_is_reported(self):
        result = self.parse_run(error=PermissionError("not executable"))
        self.assertIn("not executable", result["error"])
        self.assertTrue(scorecard.failures({"home": result}))

    def test_absent_malformed_and_incomplete_reports_fail(self):
        for report in ("", "{bad json", "[]", "null", '{"elements": 100}'):
            with self.subTest(report=report):
                result = self.parse_run("[scorecard] " + report if report else "")
                self.assertTrue(scorecard.failures({"home": result}))

    def test_invalid_metric_types_fail(self):
        for value in (None, True, "100", 100.5, -1):
            with self.subTest(value=value):
                self.assertTrue(scorecard.failures({"home": valid_result(elements=value)}))

    def test_baseline_comparison_detects_missing_pages_and_metric_regression(self):
        baseline = {"home": valid_result(), "search": valid_result()}
        worse = scorecard.regressions({"home": valid_result(elements=99)}, baseline)
        self.assertIn("search: baseline page was not run", worse)
        self.assertIn("home: elements 100 -> 99", worse)
        self.assertEqual(scorecard.regressions(baseline, baseline), [])

    def run_cli(self, result, names=(), baseline=None):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "home").mkdir()
            (root / "home" / "index.jsonl").touch()
            output = root / "results.json"
            args = ["yt_scorecard.py", "--json", str(output), *names]
            if baseline is not None:
                baseline_path = root / "baseline.json"
                baseline_path.write_text(json.dumps(baseline))
                args.extend(["--baseline", str(baseline_path)])
            with (
                patch.dict(os.environ, AURORA_CAPTURES=tmp, AURORA_BIN=sys.executable),
                patch.object(sys, "argv", args),
                patch.object(scorecard, "run_capture", return_value=result) as run,
                contextlib.redirect_stdout(io.StringIO()) as stdout,
                contextlib.redirect_stderr(io.StringIO()),
            ):
                try:
                    status = scorecard.main()
                except SystemExit as error:
                    status = error.code
                saved = json.loads(output.read_text()) if output.exists() else None
                return status, saved, stdout.getvalue(), run.call_count

    def test_failure_without_baseline_exits_nonzero_and_saves_diagnostics(self):
        status, saved, output, _ = self.run_cli(valid_result(status="timeout"))
        self.assertEqual(status, 1)
        self.assertEqual(saved["home"]["status"], "timeout")
        self.assertIn("FAILURE home: replay status timeout", output)

    def test_repeated_failure_still_fails_against_failed_baseline(self):
        result = valid_result(status=1)
        self.assertEqual(self.run_cli(result, baseline={"home": result})[0], 1)

    def test_success_without_baseline(self):
        self.assertEqual(self.run_cli(valid_result())[0], 0)

    def test_unknown_capture_is_rejected_before_any_replay(self):
        status, saved, _, calls = self.run_cli(valid_result(), names=("home", "typo"))
        self.assertEqual(status, 2)
        self.assertIsNone(saved)
        self.assertEqual(calls, 0)

    def test_missing_baseline_page_fails_default_run_but_allows_explicit_subset(self):
        baseline = {"home": valid_result(), "search": valid_result()}
        self.assertEqual(self.run_cli(valid_result(), baseline=baseline)[0], 1)
        self.assertEqual(self.run_cli(valid_result(), names=("home",), baseline=baseline)[0], 0)


if __name__ == "__main__":
    unittest.main()
