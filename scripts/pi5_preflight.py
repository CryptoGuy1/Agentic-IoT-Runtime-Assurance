#!/usr/bin/env python3
"""Check a Pi 5 software bundle and save reproducible host metadata; no GPIO access."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[1]


def read_optional(path):
    try:
        return Path(path).read_text().strip('\x00\n ')
    except OSError:
        return None


def inspect(profile='release', output_dir=None, require_pi5=False):
    model = read_optional('/proc/device-tree/model')
    checks = {
        'python_3_9_or_newer': sys.version_info >= (3, 9),
        'binaries': all(os.access(ROOT / 'target' / profile / name, os.X_OK)
                        for name in ('web_bridge', 'model_demo')),
        'dashboard_assets': all((ROOT / 'web' / name).is_file()
                                for name in ('model.html', 'model.js', 'model.css',
                                             'index.html', 'app.js', 'style.css')),
    }
    if require_pi5:
        checks['raspberry_pi_5_arm64'] = bool(model and 'Raspberry Pi 5' in model and
                                                 platform.machine() == 'aarch64')
    if output_dir is not None:
        try:
            output_dir.mkdir(parents=True, exist_ok=True)
            import tempfile
            with tempfile.TemporaryFile(dir=output_dir):
                pass
            checks['output_writable'] = True
        except OSError:
            checks['output_writable'] = False
    try:
        revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT,
                                           stderr=subprocess.DEVNULL, text=True).strip()
        dirty = bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT,
                                             stderr=subprocess.DEVNULL, text=True).strip())
    except (OSError, subprocess.CalledProcessError):
        revision, dirty = None, None
    hashes = {}
    for name in ('web_bridge', 'model_demo'):
        path = ROOT / 'target' / profile / name
        if path.is_file():
            hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    return {
        'created_at_utc': datetime.now(timezone.utc).isoformat(),
        'ready': all(checks.values()), 'checks': checks,
        'model': model, 'architecture': platform.machine(),
        'os_release': read_optional('/etc/os-release'), 'kernel': platform.release(),
        'python': platform.python_version(), 'profile': profile,
        'source_revision': revision, 'source_dirty': dirty, 'binary_sha256': hashes,
        'temperature_millidegrees_c': read_optional('/sys/class/thermal/thermal_zone0/temp'),
        'mode': 'simulation', 'gateway_connected': False,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', choices=('debug', 'release'), default='release')
    parser.add_argument('--output-dir', type=Path)
    parser.add_argument('--require-pi5', action='store_true')
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    report = inspect(args.profile, args.output_dir, args.require_pi5)
    text = json.dumps(report, indent=2) + '\n'
    if args.report:
        # Refuse to overwrite a previous measurement record.
        with args.report.open('x') as stream:
            stream.write(text)
    print(text, end='')
    return 0 if report['ready'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
