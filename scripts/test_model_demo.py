"""Closed-loop CLI artifacts, chronology and replay checks. Build binaries first."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / 'target/debug/model_demo'
SCENARIOS = ('normal', 'short-gap', 'recovery', 'uncertain', 'lost-feedback',
             'delayed-command', 'delayed-reading')


class ModelDemoTests(unittest.TestCase):
    def run_demo(self, *args):
        return subprocess.run([str(BIN), *args], capture_output=True, text=True, timeout=30)

    def test_all_artifacts_replay_and_link_releases_receipts_measurements_and_feedback(self):
        with tempfile.TemporaryDirectory() as root:
            for scenario in SCENARIOS:
                first, second = Path(root) / (scenario + '-1'), Path(root) / (scenario + '-2')
                for out in (first, second):
                    result = self.run_demo('--scenario', scenario, '--seed', '42', '--out', str(out))
                    self.assertEqual(result.returncode, 0, result.stderr)
                raw = (first / 'results.json').read_text()
                data = json.loads(raw, parse_constant=lambda value: self.fail(value))
                self.assertEqual(raw, (second / 'results.json').read_text())
                events = [json.loads(line) for line in (first / 'events.jsonl').read_text().splitlines()]
                self.assertEqual(events, data['events'])
                self.assertEqual(len(data['trace']), 81)
                self.assertEqual(data['coverage']['contained_ticks'], 81)
                self.assertEqual(data['score']['false_completion_claims'], 0)
                for command in data['score']['commands']:
                    released = next(e for e in events if e['kind'] == 'physical-command-released' and e['details']['id'] == command['id'])
                    applied = next(e for e in events if e['kind'] == 'device-receipt' and e['details']['id'] == command['id'])
                    self.assertEqual(command['released_ms'], released['at_ms'])
                    self.assertEqual(command['applied_ms'], applied['at_ms'])
                    self.assertGreater(command['applied_ms'], command['released_ms'])
                    if command['feedback_confirmed_ms'] is not None:
                        measured = next(e for e in events if e['kind'] == 'device-target-measured' and e['details']['id'] == command['id'])
                        self.assertEqual(command['target_ms'], measured['at_ms'])
                        self.assertGreater(command['feedback_confirmed_ms'], measured['at_ms'])
                if scenario == 'short-gap':
                    self.assertEqual(data['direct_pressure_status_at_proposal'], 'Unknown')
                    self.assertTrue(data['start_allowed'])
                if scenario == 'lost-feedback':
                    self.assertTrue(data['score']['objective_achieved'])
                    self.assertEqual(data['final_runtime_state'], 'Executing')
                    self.assertEqual(data['score']['physical_targets_without_confirmation'], 1)
                self.assertEqual(self.run_demo('--out', str(first)).returncode, 1)
                self.assertEqual(raw, (first / 'results.json').read_text())

    def test_invalid_arguments_never_create_artifacts(self):
        with tempfile.TemporaryDirectory() as root:
            out = Path(root) / 'bad'
            for args in [('--scenario', 'unknown'), ('--seed', '-1'), ('--unexpected', 'x')]:
                self.assertNotEqual(self.run_demo(*args, '--out', str(out)).returncode, 0)
                self.assertFalse(out.exists())
            self.assertEqual(self.run_demo('--help').returncode, 0)
            self.assertNotEqual(self.run_demo('--out').returncode, 0)

    def test_source_and_configuration_dependencies_are_exported(self):
        with tempfile.TemporaryDirectory() as root:
            out = Path(root) / 'run'
            self.assertEqual(self.run_demo('--out', str(out)).returncode, 0)
            data = json.loads((out / 'results.json').read_text())
            self.assertTrue(data['source_id'].startswith('fnv1a64:'))
            certificates = [e['details'] for e in data['events'] if e['kind'] == 'certificate-published']
            observation_ids = {o['id'] for o in data['delivered_observations']}
            command_ids = {e['details']['id'] for e in data['events'] if e['kind'] == 'physical-command-released'}
            for certificate in certificates:
                self.assertLess(certificate['checked_ms'], certificate['valid_until_ms'])
                self.assertTrue(set(certificate['observation_ids']) <= observation_ids)
                self.assertTrue(set(certificate['command_ids']) <= command_ids)
                self.assertEqual(certificate['configuration_revision'], data['configuration']['contract_revision'])


if __name__ == '__main__':
    unittest.main()
