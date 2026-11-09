#!/usr/bin/env python3
"""Local meeting demo: python3 scripts/run_web_demo.py. No third-party packages."""
import argparse
import json
import time
import signal
import select
import subprocess
import threading
import uuid
from datetime import datetime, timezone
from ollama_planner import generate, PlannerError
from model_dashboard import ModelService
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PLANNER_LOG_DIR = ROOT / 'output' / 'planner-runs'
COMMANDS = {
    'advance_200': 'advance 200ms', 'advance_500': 'advance 500ms',
    'advance_1000': 'advance 1s', 'advance_1200': 'advance 1200ms',
    'step': 'step', 'pressure_drop': 'fault pressure_safe drop',
    'pressure_restore': 'fault pressure_safe restore',
}

SENSOR_IDS = ('s1', 's2', 'thermal', 'pressure_safe', 'fan_healthy',
              'occupancy_clear', 'egress_clear', 'agent_available')
for sensor in SENSOR_IDS:
    for operation in ('drop', 'restore'):
        COMMANDS[f'{sensor}_{operation}'] = f'fault {sensor} {operation}'
COMMANDS.update({
    'hazard_on': 'plant hazard on', 'hazard_off': 'plant hazard off',
    'occupancy_occupied': 'plant occupancy occupied', 'occupancy_clear': 'plant occupancy clear',
    'egress_blocked': 'plant egress blocked', 'egress_clear': 'plant egress clear',
    'pressure_low': 'plant pressure 20', 'pressure_normal': 'plant pressure 100',
})

class Bridge:
    def __init__(self, bin_dir=None):
        self.bin_dir = Path(bin_dir) if bin_dir else ROOT / 'target/debug'
        self.lock = threading.RLock()
        self.last_proposal = None
        self.process = None
        self.failed = False
        self.start()

    def start(self):
        self.process = subprocess.Popen([str(self.bin_dir / 'web_bridge')], cwd=ROOT,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
        self.failed = False

    def close(self):
        if self.process:
            self.process.terminate()
            try:
                self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
            try:
                self.process.stdin.close()
            except OSError:
                pass  # The dead child may have left a buffered failed write.
            self.process.stdout.close()

    def exchange(self, command):
        with self.lock:
            if command.startswith(('reset ', 'reset-ai ')) and (self.failed or self.process.poll() is not None):
                self.close()
                self.start()
            if self.failed:
                raise RuntimeError('Simulator connection failed. Press Reset to start a fresh session.')
            try:
                self.process.stdin.write(command + '\n')
                self.process.stdin.flush()
                ready, _, _ = select.select([self.process.stdout], [], [], 5)
                if not ready:
                    raise RuntimeError('Simulator response timed out')
                line = self.process.stdout.readline()
                if not line:
                    raise RuntimeError('Simulator stopped. Press Reset to start a fresh session.')
                result = json.loads(line)
                if command.startswith(('reset ', 'reset-ai ')) and result['ok']:
                    self.last_proposal = None
                result['state']['planner_mode'] = result['planner_mode']
                result['state']['revision'] = result['revision']
                result['state']['planner'] = self.last_proposal
                return result
            except (OSError, ValueError, RuntimeError) as exc:
                self.failed = True
                raise RuntimeError('Simulator connection failed. Press Reset to start a fresh session.') from exc

    def plan(self, goal):
        with self.lock:
            initial = self.exchange('planning')
            if initial['planner_mode'] != 'ai':
                raise PlannerError('Select AI planner mode first; this starts a fresh session')
            proposal_id = str(uuid.uuid4())
            record = {'proposal_id': proposal_id, 'created_at': datetime.now(timezone.utc).isoformat(),
                      'goal': goal, 'context': initial['context'], 'revision': initial['revision']}
            try:
                proposal = generate(goal, initial['context'], record)
                started = {a['action']: a for a in initial['context']['history'] if a['started']}
                if any(a in started for a in proposal['actions']):
                    raise PlannerError('Proposal repeats started work. History cannot be rewritten; reset to repeat the experiment.')
                # This first S1 adapter preserves its required completed-predecessor chain.
                history = {a['action']: a for a in initial['context']['history']}
                for action, predecessor in (('fan','alarm'), ('door','fan')):
                    if action in proposal['actions']:
                        if predecessor in proposal['actions']:
                            if proposal['actions'].index(predecessor) > proposal['actions'].index(action):
                                raise PlannerError('Proposal reverses the S1 predecessor order')
                        elif history.get(predecessor, {}).get('state') != 'Completed':
                            raise PlannerError(f'{action} requires {predecessor} in the proposal or a completed predecessor')
                if proposal['decision'] == 'propose':
                    command = f"plan {initial['revision']} " + ','.join(proposal['actions'])
                    result = self.exchange(command)
                    if not result['ok']:
                        raise PlannerError(result['error'])
                    record['result'] = 'accepted'
                else:
                    result = self.exchange('state')
                    record['result'] = 'waiting'
                self.last_proposal = {'proposal_id': proposal_id, 'model': record['model'],
                    'decision': record['result'], 'actions': proposal['actions'],
                    'explanation': proposal['explanation'], 'latency_ms': record['latency_ms']}
                result['state']['planner'] = self.last_proposal
                record['state_after'] = result['state']
                return result
            except PlannerError as exc:
                record['result'] = 'rejected'
                record['error'] = str(exc)
                self.last_proposal = {'proposal_id': proposal_id, 'model': 'qwen3:1.7b',
                    'decision': 'rejected', 'actions': [], 'explanation': str(exc),
                    'latency_ms': record.get('latency_ms')}
                result = self.exchange('state')
                record['state_after'] = result['state']
                return {**result, 'ok': False, 'error': str(exc), 'recoverable': True}
            finally:
                directory = PLANNER_LOG_DIR
                directory.mkdir(parents=True, exist_ok=True)
                (directory / (proposal_id + '.json')).write_text(json.dumps(record, indent=2))


class Handler(BaseHTTPRequestHandler):
    def reply(self, status, data, mime='application/json'):
        body = json.dumps(data).encode() if mime == 'application/json' else data
        self.send_response(status)
        self.send_header('Content-Type', mime)
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Cache-Control', 'no-store')
        self.send_header('X-Content-Type-Options', 'nosniff')
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == '/health/ready':
            ready = not self.server.bridge.failed and self.server.bridge.process.poll() is None
            return self.reply(200 if ready else 503, {
                'ready': ready, 'role': 'pi5-assurance-host', 'mode': 'simulation',
                'gateway_connected': False,
                'uptime_seconds': round(time.monotonic() - self.server.started_at, 3),
            })
        if self.path == '/api/state':
            return self.call('state')
        if self.path == '/api/model/benchmark':
            try:return self.reply(200,self.server.model_service.benchmark())
            except (RuntimeError,OSError,ValueError) as exc:return self.reply(503,{'error':str(exc)})
        assets = {'/model': ('model.html','text/html; charset=utf-8'),
                  '/model.js': ('model.js','text/javascript; charset=utf-8'),
                  '/model.css': ('model.css','text/css; charset=utf-8'),
                  '/': ('index.html', 'text/html; charset=utf-8'),
                  '/app.js': ('app.js', 'text/javascript; charset=utf-8'),
                  '/style.css': ('style.css', 'text/css; charset=utf-8')}
        if self.path not in assets:
            return self.reply(404, {'error': 'Not found'})
        filename, mime = assets[self.path]
        self.reply(200, (ROOT / 'web' / filename).read_bytes(), mime)

    def do_POST(self):
        if self.path not in ('/api/control','/api/model/run'):
            return self.reply(404, {'error': 'Not found'})
        # Only our same-origin browser interface may make state-changing requests.
        if self.headers.get('Origin') not in (None, self.server.allowed_origin):
            return self.reply(403, {'error': 'Origin not allowed'})
        if self.headers.get('Host') != self.server.allowed_host:
            return self.reply(403, {'error': 'Host not allowed'})
        if self.headers.get('Content-Type', '').split(';')[0] != 'application/json':
            return self.reply(415, {'error': 'JSON request required'})
        try:
            length = int(self.headers.get('Content-Length', '0'))
            if not 0 < length <= 4096:
                raise ValueError('Invalid request size')
            obj = json.loads(self.rfile.read(length))
            if self.path == '/api/model/run':
                try:return self.reply(200,self.server.model_service.run(obj))
                except RuntimeError as exc:return self.reply(503,{'error':str(exc)})
                except subprocess.TimeoutExpired:return self.reply(503,{'error':'Experiment timed out'})
            if not isinstance(obj, dict) or set(obj) - {'control', 'preset', 'planner_mode', 'goal'}:
                raise ValueError('Invalid request')
            control = obj.get('control')
            if control == 'generate_plan':
                if set(obj) != {'control', 'goal'} or not isinstance(obj['goal'], str) or not 1 <= len(obj['goal'].strip()) <= 400:
                    raise ValueError('Provide a goal of 1–400 characters')
                try:
                    result = self.server.bridge.plan(obj['goal'].strip())
                    return self.reply(200 if result['ok'] else 422, result)
                except PlannerError as exc:
                    return self.reply(400, {'error': str(exc), 'recoverable': True})
                except RuntimeError as exc:
                    return self.reply(503, {'error': str(exc)})
            if control == 'reset':
                if set(obj) - {'control','preset','planner_mode'}:
                    raise ValueError('Invalid reset fields')
                preset = obj.get('preset', 'pressure')
                if preset not in ('pressure', 'late'):
                    raise ValueError('Unknown preset')
                mode = obj.get('planner_mode', 'scripted')
                if mode not in ('scripted', 'ai'):
                    raise ValueError('Unknown planner mode')
                command = ('reset-ai ' if mode == 'ai' else 'reset ') + preset
            else:
                if not isinstance(control, str) or control not in COMMANDS or set(obj) != {'control'}:
                    raise ValueError('Control not allowed')
                command = COMMANDS[control]
        except (ValueError, TypeError) as exc:
            return self.reply(400, {'error': str(exc)})
        self.call(command)

    def call(self, command):
        try:
            result = self.server.bridge.exchange(command)
            self.reply(200 if result['ok'] else 400, result)
        except RuntimeError as exc:
            self.reply(503, {'error': str(exc)})

    def log_message(self, *_):
        pass

def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', type=int, default=8765)
    parser.add_argument('--profile', choices=('debug', 'release'), default='debug')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--output-dir', type=Path, default=ROOT / 'output')
    args = parser.parse_args(argv)
    if not 1 <= args.port <= 65535:
        parser.error('port must be between 1 and 65535')
    return args


def main():
    global PLANNER_LOG_DIR
    args = parse_args()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    PLANNER_LOG_DIR = output_dir / 'planner-runs'
    bin_dir = ROOT / 'target' / args.profile
    if not args.skip_build:
        command = ['cargo', 'build', '--offline', '--bin', 'web_bridge', '--bin', 'model_demo']
        if args.profile == 'release':
            command.append('--release')
        subprocess.run(command, cwd=ROOT, check=True)
    for name in ('web_bridge', 'model_demo'):
        if not (bin_dir / name).is_file():
            raise SystemExit(f'Missing binary: {bin_dir / name}; build before using --skip-build')
    bridge = Bridge(bin_dir)
    try:
        server = ThreadingHTTPServer(('127.0.0.1', args.port), Handler)
        server.bridge = bridge
        server.model_service = ModelService(bin_dir, output_dir)
        server.allowed_host = f'127.0.0.1:{args.port}'
        server.allowed_origin = f'http://{server.allowed_host}'
        server.started_at = time.monotonic()
        print(f'Evidra model dashboard: {server.allowed_origin}/model\nOriginal demo: {server.allowed_origin}\nMode: simulation; Pi 4 gateway is not connected. Press Ctrl+C to stop.', flush=True)
        signal.signal(signal.SIGTERM, lambda *_: (_ for _ in ()).throw(KeyboardInterrupt()))
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        bridge.close()
        if 'server' in locals():
            server.server_close()

if __name__ == '__main__':
    main()
