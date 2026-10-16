"""Offline CLI and saved-result integration tests. Build model_v1 first."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / 'target' / 'debug' / 'model_v1'


class ModelCliTests(unittest.TestCase):
    def run_model(self, *args):
        return subprocess.run([str(BIN), *args], text=True, capture_output=True, timeout=30)

    def test_json_replay_and_existing_directory_protection(self):
        with tempfile.TemporaryDirectory() as root:
            first, second = Path(root) / 'first', Path(root) / 'second'
            for out in (first, second):
                run = self.run_model('--scenario', 'short-gap', '--seed', '42', '--out', str(out))
                self.assertEqual(run.returncode, 0, run.stderr)
            raw = (first / 'results.json').read_text()
            data = json.loads(raw, parse_constant=lambda token: self.fail(f'Non-finite JSON: {token}'))
            self.assertEqual(raw, (second / 'results.json').read_text())
            self.assertEqual(data['range_check']['decision'], 'Supported')
            self.assertEqual(data['freshness_plus_model'], 'Unknown')
            self.assertFalse(data['oracle']['outcome_confirmed'])
            self.assertEqual(data['coverage']['contained_ticks'], data['coverage']['total_ticks'])
            self.assertIsInstance(data['configuration']['estimator_dynamics'], dict)
            self.assertTrue(data['source_id'].startswith('fnv1a64:'))
            failed = self.run_model('--out', str(first))
            self.assertNotEqual(failed.returncode, 0)
            self.assertIn('already exists', failed.stderr)
            self.assertEqual(raw, (first / 'results.json').read_text())

    def test_invalid_arguments_do_not_create_results(self):
        with tempfile.TemporaryDirectory() as root:
            out = Path(root) / 'invalid'
            for args in [('--scenario', 'unknown'), ('--seed', '-1'), ('--shell', 'anything')]:
                failed = self.run_model(*args, '--out', str(out))
                self.assertNotEqual(failed.returncode, 0)
                self.assertFalse(out.exists())
            self.assertNotEqual(self.run_model('--out').returncode, 0)
            self.assertEqual(self.run_model('--help').returncode, 0)

    def test_fault_and_mismatch_reports_are_valid_and_honest(self):
        with tempfile.TemporaryDirectory() as root:
            for scenario in ['long-gap', 'conflict', 'mismatch', 'door-jam', 'delayed-reading', 'occupancy-entry']:
                out = Path(root) / scenario
                run = self.run_model('--scenario', scenario, '--out', str(out))
                self.assertEqual(run.returncode, 0, run.stderr)
                data = json.loads((out / 'results.json').read_text())
                self.assertEqual(len(data['trace']), data['coverage']['total_ticks'])
                if scenario in ('long-gap', 'conflict', 'occupancy-entry'):
                    self.assertEqual(data['range_check']['decision'], 'Unknown')
                if scenario == 'mismatch':
                    self.assertLess(data['coverage']['contained_ticks'], data['coverage']['total_ticks'])
                if scenario == 'door-jam':
                    self.assertFalse(data['oracle']['physical_success'])


if __name__ == '__main__':
    unittest.main()
