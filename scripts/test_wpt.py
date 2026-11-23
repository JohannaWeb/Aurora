"""Checks for the WPT runner's parsing, discovery and expectation comparison."""

import json
import tempfile
import unittest
from pathlib import Path

import wpt


def report(tests, complete=True, harness=0):
    return "noise\n[wpt] " + json.dumps(
        {"complete": complete, "harness": {"status": harness, "message": None}, "tests": tests}
    ) + "\nmore noise\n"


class ParseReportTests(unittest.TestCase):
    def test_maps_statuses(self):
        result = wpt.parse_report(report([
            {"name": "a", "status": 0}, {"name": "b", "status": 1}, {"name": "c", "status": 2},
        ]))
        self.assertEqual(result, {"harness": "OK", "subtests": {"a": "PASS", "b": "FAIL", "c": "TIMEOUT"}})

    def test_incomplete_run_is_a_timeout_that_keeps_finished_subtests(self):
        result = wpt.parse_report(report([{"name": "a", "status": 0}], complete=False))
        self.assertEqual(result, {"harness": "TIMEOUT", "subtests": {"a": "PASS"}})

    def test_missing_or_broken_report_is_a_crash(self):
        self.assertEqual(wpt.parse_report("nothing here")["harness"], "CRASH")
        self.assertEqual(wpt.parse_report("[wpt] {not json")["harness"], "CRASH")
        self.assertEqual(wpt.parse_report('[wpt] {"missing": true}')["harness"], "CRASH")

    def test_report_line_may_contain_unicode_line_breaks(self):
        # JSON.stringify leaves these unescaped; str.splitlines() splits on them.
        name = "class \u2028 \u2029 \u0085 \x0b \x0c"
        line = "[wpt] " + json.dumps(
            {"complete": True, "harness": {"status": 0, "message": None},
             "tests": [{"name": name, "status": 1, "message": name}]},
            ensure_ascii=False,
        )
        result = wpt.parse_report("noise\r\n" + line + "\r\nmore noise\n")
        self.assertEqual(result, {"harness": "OK", "subtests": {name: "FAIL"}})

    def test_harness_error(self):
        self.assertEqual(wpt.parse_report(report([], harness=1))["harness"], "ERROR")


class MetaTests(unittest.TestCase):
    def test_parses_header_only(self):
        meta = wpt.parse_meta("// META: title=T\n// META: script=a.js\n//META: script=b.js\ntest();\n// META: script=late.js\n")
        self.assertEqual(meta, {"title": ["T"], "script": ["a.js", "b.js"]})

    def test_window_scope(self):
        self.assertTrue(wpt.runs_in_window({}))
        self.assertTrue(wpt.runs_in_window({"global": ["window,worker"]}))
        self.assertTrue(wpt.runs_in_window({"global": ["default"]}))
        self.assertFalse(wpt.runs_in_window({"global": ["worker"]}))
        self.assertFalse(wpt.runs_in_window({"global": ["dedicatedworker", "shadowrealm"]}))

    def test_wrapper_loads_harness_meta_scripts_then_test(self):
        page = wpt.wrapper_page("dom/x.any.js", {"script": ["helper.js"], "title": ["X"]})
        order = [page.index(s) for s in ("testharness.js", "testharnessreport.js", "helper.js", "/dom/x.any.js")]
        self.assertEqual(order, sorted(order))
        self.assertIn("<title>X</title>", page)


class DiscoverTests(unittest.TestCase):
    def test_finds_harness_pages_and_window_any_tests(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            files = {
                "dom/nodes/a.html": '<script src="/resources/testharness.js"></script>',
                "dom/nodes/reftest.html": "<p>no harness</p>",
                "dom/nodes/a-ref.html": '<script src="/resources/testharness.js"></script>',
                "dom/nodes/b-manual.html": '<script src="/resources/testharness.js"></script>',
                "dom/nodes/resources/helper.html": '<script src="/resources/testharness.js"></script>',
                "dom/nodes/c.any.js": "test(() => {});",
                "dom/nodes/d.any.js": "// META: global=worker\ntest(() => {});",
                "dom/nodes/e.window.js": "test(() => {});",
                "dom/nodes/f.worker.js": "test(() => {});",
            }
            for rel, body in files.items():
                (root / rel).parent.mkdir(parents=True, exist_ok=True)
                (root / rel).write_text(body)
            found = wpt.discover(root, [])
            self.assertEqual(found, ["dom/nodes/a.html", "dom/nodes/c.any.html", "dom/nodes/e.window.html"])
            self.assertEqual(wpt.discover(root, ["dom/nodes/c"]), ["dom/nodes/c.any.html"])


class CompareTests(unittest.TestCase):
    def setUp(self):
        self.expected = wpt.to_expectations({
            "t.html": {"harness": "OK", "subtests": {"a": "PASS", "b": "FAIL"}},
        })

    def test_unchanged_is_clean(self):
        worse, better = wpt.compare({"t.html": {"harness": "OK", "subtests": {"a": "PASS", "b": "FAIL"}}}, self.expected)
        self.assertEqual((worse, better), ([], []))

    def test_lost_pass_is_a_regression(self):
        worse, _ = wpt.compare({"t.html": {"harness": "OK", "subtests": {"a": "FAIL", "b": "FAIL"}}}, self.expected)
        self.assertEqual(len(worse), 1)
        self.assertIn("'a'", worse[0])

    def test_vanished_passing_subtest_is_a_regression(self):
        worse, _ = wpt.compare({"t.html": {"harness": "CRASH", "subtests": {}}}, self.expected)
        self.assertEqual(len(worse), 2)  # harness got worse, and 'a' is missing

    def test_flaky_tests_are_not_compared(self):
        self.expected["t.html"]["flaky"] = True
        worse, better = wpt.compare({"t.html": {"harness": "TIMEOUT", "subtests": {}}}, self.expected)
        self.assertEqual((worse, better), ([], []))

    def test_new_pass_is_an_improvement(self):
        worse, better = wpt.compare({"t.html": {"harness": "OK", "subtests": {"a": "PASS", "b": "PASS"}}}, self.expected)
        self.assertEqual(worse, [])
        self.assertEqual(len(better), 1)


if __name__ == "__main__":
    unittest.main()
