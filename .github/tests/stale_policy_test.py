"""Offline configuration regression. Does not query or mutate GitHub items."""
import pathlib
import unittest
import yaml

ROOT = pathlib.Path(__file__).resolve().parents[2]
POLICY = yaml.safe_load((ROOT / '.github/workflows/stale.yml').read_text(encoding='utf-8'))


class StalePolicy(unittest.TestCase):
    def options(self, job):
        return POLICY['jobs'][job]['steps'][0]['with']

    def test_issue_type_isolation_and_notice_window(self):
        options = self.options('stale-issues')
        self.assertEqual(options['days-before-pr-stale'], -1)
        self.assertEqual(options['days-before-pr-close'], -1)
        self.assertEqual(options['days-before-issue-close'], 14)
        self.assertIn('60', options['days-before-issue-stale'])
        self.assertIn('-1', options['days-before-issue-stale'])
        self.assertNotIn('if', POLICY['jobs']['stale-issues'])

    def test_pr_type_isolation_and_draft_exemption(self):
        options = self.options('stale-prs')
        self.assertEqual(options['days-before-issue-stale'], -1)
        self.assertEqual(options['days-before-issue-close'], -1)
        self.assertEqual(options['days-before-pr-stale'], 30)
        self.assertEqual(options['days-before-pr-close'], -1)
        self.assertIs(options['exempt-draft-pr'], True)

    def test_no_other_closing_job_or_generic_zero_days(self):
        for job in POLICY['jobs'].values():
            for step in job.get('steps', []):
                options = step.get('with', {})
                self.assertNotIn('days-before-close', options)
                self.assertNotIn('days-before-stale', options)
                self.assertNotEqual(options.get('days-before-issue-close'), 0)
                self.assertNotEqual(options.get('days-before-pr-close'), 0)
        self.assertNotIn('close-stale', POLICY['jobs'])

    def test_maintained_pins_permissions_and_dry_run(self):
        self.assertEqual(POLICY['permissions'], {
            'contents': 'read', 'issues': 'write', 'pull-requests': 'write'})
        for job in ['stale-issues', 'stale-prs']:
            step = POLICY['jobs'][job]['steps'][0]
            self.assertEqual(step['uses'], 'actions/stale@eb5cf3af3ac0a1aa4c9c45633dd1ae542a27a899')
            self.assertIn("operation == 'dry-run'", step['with']['debug-only'])


if __name__ == '__main__':
    unittest.main()
