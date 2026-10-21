import json
import subprocess
import threading
import unittest
import urllib.request
import urllib.error
from http.server import ThreadingHTTPServer
from pathlib import Path
from run_web_demo import Handler
from model_dashboard import ModelService
from run_refinement_benchmarks import wilson, oracle_summary
ROOT=Path(__file__).resolve().parents[1]
class RefinementTests(unittest.TestCase):
    def test_oracle_v2_metrics_keep_denominators_and_null_zero_denominators(self):
        rows=[
          {'oracle_label':'SafeUseful','policy_response':'Withheld','target_achieved':None},
          {'oracle_label':'Unsafe','policy_response':'Released','target_achieved':True}]
        score=oracle_summary(rows)
        self.assertEqual(score['unnecessary_block_rate'],{'numerator':1,'denominator':1,'rate':1.0})
        self.assertEqual(score['unsafe_release_rate'],{'numerator':1,'denominator':1,'rate':1.0})
        self.assertIsNone(score['safe_unnecessary_release_rate']['rate'])
    def test_reference_corpus_is_balanced_and_outcome_handling_stays_required(self):
        data=json.loads(subprocess.check_output([str(ROOT/'target/debug/policy_cases')],text=True))
        self.assertEqual(len(data['cases']),32)
        for category in ('Start','Run','Commit','Outcome'):
            self.assertEqual(sum(c['category']==category for c in data['cases']),8)
        for row in data['cases']:
            if row['category']=='Outcome':self.assertTrue(all(response==row['expected'] for response in row['responses'].values()))
        self.assertFalse(data['population_inference'])
    def test_zero_event_uncertainty_is_not_reported_as_zero_risk(self):
        low,high=wilson(0,16);self.assertAlmostEqual(low,0);self.assertGreater(high,.15)
        self.assertIsNone(wilson(0,0))
    def test_dashboard_runs_rust_and_serves_model_assets_and_validates_input(self):
        server=ThreadingHTTPServer(('127.0.0.1',0),Handler);server.model_service=ModelService()
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        base=f'http://127.0.0.1:{server.server_port}'
        try:
            for path,mime in [('/model','text/html'),('/model.js','text/javascript'),('/model.css','text/css')]:
                with urllib.request.urlopen(base+path) as response:self.assertIn(mime,response.headers['Content-Type'])
            def request(body,origin='http://127.0.0.1:8765'):
                return urllib.request.Request(base+'/api/model/run',data=json.dumps(body).encode(),headers={'Host':'127.0.0.1:8765','Origin':origin,'Content-Type':'application/json'})
            with urllib.request.urlopen(request({'scenario':'short-gap','policy':'B6','seed':42})) as response:data=json.load(response)
            self.assertEqual(data['policy_schema'],'refinement-v1');self.assertEqual(data['final_runtime_state'],'Completed')
            self.assertEqual(len(data['trace']),121)
            for body in [{'scenario':'../bad'},{'seed':-1},{'seed':True},{'autonomous':'yes'},{'shell':'anything'}]:
                with self.assertRaises(urllib.error.HTTPError) as error:urllib.request.urlopen(request(body))
                self.assertEqual(error.exception.code,400)
            with self.assertRaises(urllib.error.HTTPError) as error:urllib.request.urlopen(request({},'https://example.org'))
            self.assertEqual(error.exception.code,403)
        finally:server.shutdown();server.server_close();thread.join(timeout=5)
