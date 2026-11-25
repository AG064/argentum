# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064
"""Tests for the parts of argentum_pr.py that need no Argentum, Git or network."""

import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import argentum_pr  # noqa: E402

PR_DIFF = """diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 fn a() {}
-fn b() {}
+fn b() { 1 }
+fn c() { 2 }
 fn d() {}
"""


class Suggestions(unittest.TestCase):
    def test_commentable_lines_are_the_new_side_of_each_hunk(self):
        self.assertEqual(argentum_pr.commentable_lines(PR_DIFF), {"src/lib.rs": {1, 2, 3, 4}})

    def test_a_replacement_inside_the_diff_becomes_a_suggestion(self):
        fix = ("--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,2 +2,2 @@\n"
               "-fn b() { 1 }\n-fn c() { 2 }\n+fn b() { 10 }\n+fn c() { 20 }\n")
        comments, skipped = argentum_pr.suggestions(
            fix, argentum_pr.commentable_lines(PR_DIFF), {})
        self.assertEqual(skipped, 0)
        self.assertEqual(comments, [{
            "path": "src/lib.rs", "line": 3, "side": "RIGHT", "start_line": 2,
            "start_side": "RIGHT",
            "body": "```suggestion\nfn b() { 10 }\nfn c() { 20 }\n```"}])

    def test_an_insertion_keeps_the_line_it_follows(self):
        fix = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,0 +2 @@\n+fn z() {}\n"
        head = {"src/lib.rs": ["fn a() {}", "fn b() { 1 }", "fn c() { 2 }", "fn d() {}"]}
        comments, skipped = argentum_pr.suggestions(
            fix, argentum_pr.commentable_lines(PR_DIFF), head)
        self.assertEqual(skipped, 0)
        self.assertEqual(comments[0]["line"], 1)
        self.assertEqual(comments[0]["body"], "```suggestion\nfn a() {}\nfn z() {}\n```")

    def test_a_deletion_suggests_nothing_in_place(self):
        fix = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3 +2,0 @@\n-fn c() { 2 }\n"
        comments, _ = argentum_pr.suggestions(fix, argentum_pr.commentable_lines(PR_DIFF), {})
        self.assertEqual(comments[0]["body"], "```suggestion\n\n```")

    def test_changes_outside_the_pull_request_lines_are_skipped(self):
        fix = ("--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -9 +9 @@\n-old\n+new\n"
               "--- a/other.rs\n+++ b/other.rs\n@@ -1 +1 @@\n-x\n+y\n")
        comments, skipped = argentum_pr.suggestions(
            fix, argentum_pr.commentable_lines(PR_DIFF), {})
        self.assertEqual((comments, skipped), ([], 2))


class Text(unittest.TestCase):
    def test_mentions_and_the_marker_are_neutralized(self):
        text = argentum_pr.neutralize(f"ping @owner and {argentum_pr.MARKER} but not a@b.c `@x`")
        self.assertNotIn("@owner", text)
        self.assertIn("@​owner", text)
        self.assertNotIn(argentum_pr.MARKER, text)
        self.assertIn("a@b.c", text)
        self.assertIn("`@x`", text)

    def test_a_fence_is_longer_than_any_backtick_run_inside(self):
        self.assertTrue(argentum_pr.fenced("a ```` b").startswith("`````text\n"))

    def test_bounded_text_says_where_it_was_cut(self):
        text, cut = argentum_pr.bounded("x" * 20, 10, "diff")
        self.assertTrue(cut)
        self.assertIn("[diff cut at 10 bytes]", text)

    def test_a_missing_answer_reports_why(self):
        result = {"run_state": "failed", "answer": {"available": False},
                  "failure": {"code": "approval_required"}}
        self.assertIn("approval_required", argentum_pr.answer_text(result))


if __name__ == "__main__":
    unittest.main()
