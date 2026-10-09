#!/usr/bin/env python3
"""Local meeting demo: python3 scripts/run_web_demo.py. No third-party packages."""
import json
import signal
import select
import subprocess
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
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
    def __init__(self):
        self.lock = threading.Lock()
        self.process = None
        self.failed = False
        self.start()

    def start(self):
        self.process = subprocess.Popen([str(ROOT / 'target/debug/web_bridge')], cwd=ROOT,
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
            if command.startswith('reset ') and (self.failed or self.process.poll() is not None):
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
                return json.loads(line)
            except (OSError, ValueError, RuntimeError) as exc:
                self.failed = True
                raise RuntimeError('Simulator connection failed. Press Reset to start a fresh session.') from exc

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
        if self.path == '/api/state':
            return self.call('state')
        assets = {'/': ('index.html', 'text/html; charset=utf-8'),
                  '/app.js': ('app.js', 'text/javascript; charset=utf-8'),
                  '/style.css': ('style.css', 'text/css; charset=utf-8')}
        if self.path not in assets:
            return self.reply(404, {'error': 'Not found'})
        filename, mime = assets[self.path]
        self.reply(200, (ROOT / 'web' / filename).read_bytes(), mime)

    def do_POST(self):
        if self.path != '/api/control':
            return self.reply(404, {'error': 'Not found'})
        # Only our same-origin browser interface may make state-changing requests.
        if self.headers.get('Origin') not in (None, 'http://127.0.0.1:8765'):
            return self.reply(403, {'error': 'Origin not allowed'})
        if self.headers.get('Host') != '127.0.0.1:8765':
            return self.reply(403, {'error': 'Host not allowed'})
        if self.headers.get('Content-Type', '').split(';')[0] != 'application/json':
            return self.reply(415, {'error': 'JSON request required'})
        try:
            length = int(self.headers.get('Content-Length', '0'))
            if not 0 < length <= 1024:
                raise ValueError('Invalid request size')
            obj = json.loads(self.rfile.read(length))
            if not isinstance(obj, dict) or set(obj) - {'control', 'preset'}:
                raise ValueError('Invalid request')
            control = obj.get('control')
            if control == 'reset':
                preset = obj.get('preset', 'pressure')
                if preset not in ('pressure', 'late'):
                    raise ValueError('Unknown preset')
                command = 'reset ' + preset
            else:
                if not isinstance(control, str) or control not in COMMANDS or 'preset' in obj:
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

def main():
    subprocess.run(['cargo', 'build', '--offline', '--bin', 'web_bridge'], cwd=ROOT, check=True)
    bridge = Bridge()
    try:
        server = ThreadingHTTPServer(('127.0.0.1', 8765), Handler)
        server.bridge = bridge
        print('DCRA web demo: http://127.0.0.1:8765\nVirtual time starts paused. Press Ctrl+C to stop.', flush=True)
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
