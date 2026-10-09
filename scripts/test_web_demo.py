#!/usr/bin/env python3
"""Bridge protocol regression tests; build web_bridge before running."""
import json
import subprocess
import unittest
from run_web_demo import Bridge
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
class DemoTests(unittest.TestCase):
    def setUp(self):
        self.p = subprocess.Popen([str(ROOT/'target/debug/web_bridge')], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    def tearDown(self):
        self.p.stdin.close(); self.p.wait(timeout=5); self.p.stdout.close()
    def send(self, command):
        self.p.stdin.write(command+'\n'); self.p.stdin.flush()
        return json.loads(self.p.stdout.readline())
    def action(self, state, action):
        return next(a for a in state['actions'] if a['action']==action)
    def test_pressure_flow_and_restore(self):
        s=self.send('advance 500ms')['state']
        self.assertEqual(s['time_ms'],500)
        self.assertEqual(self.action(s,'fan')['state'],'Executing')
        self.assertTrue(s['plant']['fan_starting'])
        s=self.send('fault pressure_safe drop')['state']
        self.assertEqual(self.action(s,'fan')['state'],'Aborting')
        self.assertEqual(s['plant']['pressure'],100)
        self.assertEqual(next(e for e in s['evidence'] if e['id']=='pressure_safe')['status'],'Unknown')
        s=self.send('advance 200ms')['state']
        self.assertEqual(self.action(s,'fan')['state'],'Cancelled')
        self.assertFalse(s['plant']['fan_starting'] or s['plant']['fan_high'])
        self.assertEqual(self.action(s,'door')['state'],'Pending')
        s=self.send('fault pressure_safe restore')['state']
        self.assertEqual(self.action(s,'fan')['state'],'Cancelled')
        self.assertLessEqual(len(s['events']),30)
    def test_late_permission_and_reset(self):
        self.send('reset late')
        s=self.send('advance 1200ms')['state']
        self.assertEqual(s['preset'],'late')
        self.assertTrue(all(a['state'] not in ('Executing','Completed','Committed') for a in s['actions']))
        self.assertFalse(s['plant']['fan_starting'] or s['plant']['fan_high'] or s['plant']['alarm_active'])
        self.assertTrue(any(e['kind']=='AuthorityRejected' for e in s['events']))
        s=self.send('reset pressure')['state']
        self.assertEqual(s['time_ms'],0)
        self.assertTrue(all(a['state']=='Pending' for a in s['actions']))
    def test_thermal_alternative_support(self):
        self.send('fault thermal drop')
        s=self.send('advance 500ms')['state']
        self.assertEqual(self.action(s,'fan')['state'],'Executing')
        self.send('reset pressure')
        self.send('fault thermal drop')
        self.send('fault s1 drop')
        s=self.send('advance 500ms')['state']
        self.assertEqual(self.action(s,'fan')['state'],'Pending')
        self.assertTrue(s['plant']['hazard'])
        s=self.send('fault thermal restore')['state']
        self.assertEqual(self.action(s,'fan')['state'],'Executing')

    def test_building_changes_wait_for_observations(self):
        self.send('advance 500ms')
        s=self.send('plant occupancy occupied')['state']
        self.assertTrue(s['plant']['occupied'])
        self.assertEqual(next(e for e in s['evidence'] if e['id']=='occupancy_clear')['status'],'Valid')
        self.send('plant egress blocked')
        self.send('plant pressure 20')
        s=self.send('advance 500ms')['state']
        self.assertFalse(s['plant']['egress_clear'])
        for sensor in ('occupancy_clear', 'egress_clear', 'pressure_safe'):
            self.assertEqual(next(e for e in s['evidence'] if e['id']==sensor)['status'],'Invalid')
        self.assertIn(self.action(s,'fan')['state'], ('Aborting','Cancelled'))

    def test_all_sensor_controls_and_whitelist(self):
        from run_web_demo import COMMANDS, SENSOR_IDS
        for sensor in SENSOR_IDS:
            s=self.send(COMMANDS[sensor+'_drop'])['state']
            self.assertEqual(next(e for e in s['evidence'] if e['id']==sensor)['status'],'Unknown')
            self.assertTrue(self.send(COMMANDS[sensor+'_restore'])['ok'])
        before=self.send('state')['state']
        for command in ('fault permission drop', 'plant pressure 999', 'plant unknown on'):
            self.assertFalse(self.send(command)['ok'])
        self.assertEqual(before,self.send('state')['state'])

    def test_invalid_control_is_atomic(self):
        before=self.send('state')['state']
        self.assertFalse(self.send('save /tmp/not-allowed')['ok'])
        self.assertFalse(self.send('reset unknown')['ok'])
        self.assertEqual(before,self.send('state')['state'])
class RecoveryTests(unittest.TestCase):
    def test_bridge_restart_after_failure(self):
        bridge = Bridge()
        try:
            bridge.process.terminate()
            bridge.process.wait(timeout=5)
            with self.assertRaises(RuntimeError):
                bridge.exchange('state')
            fresh = bridge.exchange('reset late')
            self.assertTrue(fresh['ok'])
            self.assertEqual(fresh['state']['time_ms'], 0)
            self.assertEqual(fresh['state']['preset'], 'late')
        finally:
            bridge.close()

if __name__=='__main__': unittest.main()
