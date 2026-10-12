"""Offline integration tests. Live inference is separately rehearsed in the browser."""
import json
import subprocess
import threading
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path
from run_web_demo import Bridge
from ollama_planner import validate_proposal, PlannerError
ROOT = Path(__file__).resolve().parents[1]

def proposal(actions, decision='propose'):
    return {'decision':decision,'actions':actions,'explanation':'Test proposal'}

def fake_generate(actions, decision='propose'):
    def generate(goal, context, record):
        record.update(model='qwen3:1.7b',latency_ms=1,normalized_proposal=proposal(actions,decision))
        return record['normalized_proposal']
    return generate

class PlannerTests(unittest.TestCase):
    def setUp(self):
        self.logs = tempfile.TemporaryDirectory(dir=ROOT/'tmp')
        self.log_patch = patch('run_web_demo.PLANNER_LOG_DIR', Path(self.logs.name))
        self.log_patch.start()
        self.bridge = Bridge()
        self.bridge.exchange('reset-ai pressure')
    def tearDown(self):
        self.bridge.close()
        self.log_patch.stop()
        self.logs.cleanup()
    def state(self):
        return self.bridge.exchange('state')['state']
    def test_observation_only_empty_ai_session(self):
        result = self.bridge.exchange('planning')
        self.assertEqual(result['state']['time_ms'],0)
        self.assertFalse(any(a['current'] for a in result['state']['actions']))
        self.assertNotIn('plant',result['context'])
        self.assertNotIn('events',result['context'])
        self.bridge.exchange('fault pressure_safe drop')
        self.bridge.exchange('plant pressure 20')
        c=self.bridge.exchange('planning')['context']
        self.assertEqual(next(e for e in c['evidence'] if e['id']=='pressure_safe')['status'],'Unknown')
    def test_generated_sequence_executes_and_evidence_stops(self):
        with patch('run_web_demo.generate',fake_generate(['alarm','fan','door'])):
            r=self.bridge.plan('Respond to hazard')
        self.assertTrue(r['ok'])
        self.assertEqual(r['state']['time_ms'],0)
        self.assertFalse(r['state']['plant']['alarm_active'])
        r=self.bridge.exchange('advance 500ms')
        fan=next(a for a in r['state']['actions'] if a['current'] and a['action']=='fan')
        self.assertEqual(fan['state'],'Executing')
        self.bridge.exchange('fault pressure_safe drop')
        r=self.bridge.exchange('advance 200ms')
        fan=next(a for a in r['state']['actions'] if a['current'] and a['action']=='fan')
        self.assertEqual(fan['state'],'Cancelled')
        with patch('run_web_demo.generate',fake_generate(['door'])):
            self.assertFalse(self.bridge.plan('Close door')['ok'])
    def test_wait_and_failure_leave_plan_unchanged(self):
        before=self.state()
        with patch('run_web_demo.generate',fake_generate([], 'wait')):
            self.assertTrue(self.bridge.plan('Wait')['ok'])
        with patch('run_web_demo.generate',side_effect=PlannerError('Ollama unavailable')):
            r=self.bridge.plan('Respond')
        self.assertFalse(r['ok'])
        self.assertTrue(r['recoverable'])
        logs=list(Path(self.logs.name).glob('*.json'))
        self.assertEqual(len(logs),2)
        self.assertTrue(any(json.loads(p.read_text())['result']=='rejected' for p in logs))
        after=self.state()
        for key in ('time_ms','actions','evidence','plant','revision'):
            self.assertEqual(before[key],after[key])
    def test_rust_rejects_stale_unknown_duplicate_and_reversed(self):
        rev=self.state()['revision']
        before=self.state()
        for names in ('door','fan,alarm','alarm,alarm','shell'):
            self.assertFalse(self.bridge.exchange(f'plan {rev} {names}')['ok'])
        self.bridge.exchange('advance 200ms')
        self.assertFalse(self.bridge.exchange(f'plan {rev} alarm')['ok'])
        self.assertEqual(before['plant'],self.state()['plant'])
    def test_missing_evidence_blocks_accepted_plan(self):
        self.bridge.exchange('fault fan_healthy drop')
        with patch('run_web_demo.generate',fake_generate(['alarm','fan','door'])):
            self.assertTrue(self.bridge.plan('Respond')['ok'])
        r=self.bridge.exchange('advance 500ms')
        fan=next(a for a in r['state']['actions'] if a['current'] and a['action']=='fan')
        self.assertEqual(fan['state'],'Pending')
        self.assertFalse(r['state']['plant']['fan_starting'])
    def test_completed_history_and_suffix_dependencies(self):
        with patch('run_web_demo.generate',fake_generate(['alarm'])):
            self.bridge.plan('Warn')
        self.bridge.exchange('advance 500ms')
        with patch('run_web_demo.generate',fake_generate(['alarm','fan'])):
            self.assertFalse(self.bridge.plan('Repeat alarm')['ok'])
        with patch('run_web_demo.generate',fake_generate(['fan','door'])):
            self.assertTrue(self.bridge.plan('Ventilate then close')['ok'])
        for _ in range(6):
            r=self.bridge.exchange('advance 1s')
        self.assertTrue(r['state']['plant']['door_closed'])
    def test_inference_serializes_controls(self):
        entered=threading.Event(); release=threading.Event(); completed=threading.Event()
        def slow(goal, context, record):
            entered.set(); release.wait(3)
            return fake_generate(['alarm'])(goal,context,record)
        with patch('run_web_demo.generate',slow):
            planning=threading.Thread(target=lambda:self.bridge.plan('Warn'))
            planning.start(); self.assertTrue(entered.wait(2))
            control=threading.Thread(target=lambda:(self.bridge.exchange('advance 200ms'),completed.set()))
            control.start(); self.assertFalse(completed.wait(.05))
            release.set();planning.join(5);control.join(5)
            self.assertTrue(completed.is_set())
            self.assertEqual(self.state()['time_ms'],200)

class SchemaTests(unittest.TestCase):
    def test_invalid_schemas(self):
        for obj in ({},proposal(['shell']),proposal(['alarm','alarm']),proposal([]),proposal(['alarm'],'wait'),{**proposal(['alarm']),'command':'x'}):
            with self.assertRaises(PlannerError): validate_proposal(obj)
        self.assertEqual(validate_proposal(proposal(['alarm']))['actions'],['alarm'])

if __name__ == '__main__': unittest.main()
