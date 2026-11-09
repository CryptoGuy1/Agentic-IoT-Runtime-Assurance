"""Deployment regressions using the actual HTTP service and Rust binaries."""
import http.client
import json
from pathlib import Path
import tempfile
import threading
import time
import unittest
from http.server import ThreadingHTTPServer
from unittest.mock import patch

from model_dashboard import ModelService
from pi5_preflight import inspect
from run_web_demo import Bridge, Handler, parse_args


class DeploymentTests(unittest.TestCase):
    def test_preflight_does_not_claim_pi_on_laptop(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch('pi5_preflight.read_optional', return_value=None):
                report = inspect('debug', Path(directory), require_pi5=True)
            self.assertFalse(report['ready'])
            self.assertFalse(report['checks']['raspberry_pi_5_arm64'])
            self.assertTrue(report['checks']['output_writable'])
            self.assertEqual(report['mode'], 'simulation')
            self.assertFalse(report['gateway_connected'])

    def test_cli_configuration(self):
        args = parse_args(['--profile', 'release', '--skip-build', '--port', '9876'])
        self.assertEqual(args.profile, 'release')
        self.assertTrue(args.skip_build)
        self.assertEqual(args.port, 9876)
        with self.assertRaises(SystemExit):
            parse_args(['--port', '0'])

    def test_health_experiment_origin_and_child_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
            bridge = Bridge()
            server.bridge = bridge
            server.model_service = ModelService(output_dir=Path(directory))
            server.allowed_host = f'127.0.0.1:{server.server_port}'
            server.allowed_origin = f'http://{server.allowed_host}'
            server.started_at = time.monotonic()
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            connection = http.client.HTTPConnection('127.0.0.1', server.server_port, timeout=40)
            try:
                connection.request('GET', '/health/ready')
                response = connection.getresponse()
                self.assertEqual(response.status, 200)
                self.assertFalse(json.loads(response.read())['gateway_connected'])
                body = json.dumps({'scenario': 'short-gap', 'policy': 'B6', 'seed': 42})
                connection.request('POST', '/api/model/run', body,
                                   {'Content-Type': 'application/json', 'Origin': 'http://untrusted.example'})
                response = connection.getresponse()
                self.assertEqual(response.status, 403)
                response.read()
                connection.request('POST', '/api/model/run', body,
                                   {'Content-Type': 'application/json', 'Origin': server.allowed_origin})
                response = connection.getresponse()
                self.assertEqual(response.status, 200)
                data = json.loads(response.read())
                self.assertTrue((Path(directory) / 'model-dashboard' / data['run_id'] / 'results.json').exists())
                bridge.process.terminate()
                bridge.process.wait(timeout=5)
                connection.request('GET', '/health/ready')
                response = connection.getresponse()
                self.assertEqual(response.status, 503)
                self.assertFalse(json.loads(response.read())['ready'])
            finally:
                connection.close()
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)
                bridge.close()


if __name__ == '__main__':
    unittest.main()
