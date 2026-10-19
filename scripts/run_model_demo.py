#!/usr/bin/env python3
"""Build and run the seven deterministic closed-loop demonstrations offline."""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SCENARIOS = ('normal', 'short-gap', 'recovery', 'uncertain', 'lost-feedback',
             'delayed-command', 'delayed-reading')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True, help='new directory; parent must exist')
    parser.add_argument('--seed', type=int, default=42)
    args = parser.parse_args()
    if not 0 <= args.seed <= 2**64 - 1:
        parser.error('seed must be an unsigned 64-bit integer')
    out = args.out.resolve()
    if out.exists() or not out.parent.is_dir():
        parser.error('choose a new output directory with an existing parent')
    subprocess.run(['cargo', 'build', '--offline', '--bin', 'model_demo'], cwd=ROOT, check=True)
    out.mkdir()
    rows = []
    manifest = []
    for name in SCENARIOS:
        destination = out / name
        subprocess.run([str(ROOT / 'target/debug/model_demo'), '--scenario', name,
                        '--seed', str(args.seed), '--out', str(destination)], check=True)
        data = json.loads((destination / 'results.json').read_text())
        score = data['score']
        rows.append(f"| [{name}]({name}/report.md) | {data['start_allowed']} | "
                    f"{data['final_runtime_state']} | {len(score['commands'])} | "
                    f"{score['objective_achieved']} | {score['physical_targets_without_confirmation']} | "
                    f"{data['coverage']['contained_ticks']}/{data['coverage']['total_ticks']} |")
        manifest.append({'scenario': name, 'seed': args.seed, 'source_id': data['source_id'],
                         'config_id': data['config_id'], 'results': f'{name}/results.json'})
    (out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    (out / 'report.md').write_text(
        '# End-to-end model demonstration\n\n'
        'The model-backed runtime, simulated plant, command channel, measured feedback and '
        'post-run independent scorer now operate in one closed loop. These are demonstration '
        'fixtures, not a B5/B6 policy ranking or an 80% correctness result.\n\n'
        '| Scenario | START allowed | Final task state | Commands | Goal achieved | '
        'Physical targets without confirmation | State containment |\n'
        '| --- | --- | --- | --- | --- | --- | --- |\n' + '\n'.join(rows) + '\n\n'
        'Read normal, short-gap and recovery first. Lost-feedback separates physical success '
        'from runtime confirmation. Delayed-command can cause early recovery because delivery '
        'reduces recovery headroom. Each scenario saves report.md, results.json and events.jsonl. '
        'The manifest identifies source/configuration fingerprints and seeds.\n')
    print(f'Review: {out / "report.md"}')


if __name__ == '__main__':
    main()
