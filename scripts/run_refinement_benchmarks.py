#!/usr/bin/env python3
"""Frozen decision corpus and matched pilot/confirmatory closed-loop policies."""
import argparse
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import random
import shutil
import statistics
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SCENARIOS = ('normal','short-gap','recovery','uncertain','lost-feedback','delayed-command','delayed-reading')
POLICIES = tuple('B'+str(i) for i in range(7))

def source_paths(schema='refinement-v1'):
    protocol='refinement-oracle-v2-protocol.md' if schema=='refinement-v2' else 'refinement-benchmark-protocol.md'
    return sorted((ROOT/'src').rglob('*.rs')) + [ROOT/'Cargo.toml', ROOT/'Cargo.lock', Path(__file__).resolve(), ROOT/'docs'/protocol]

def fingerprint(schema='refinement-v1'):
    paths = source_paths(schema)
    digest=hashlib.sha256()
    for path in paths:
        digest.update(str(path.relative_to(ROOT)).encode());digest.update(path.read_bytes())
    return digest.hexdigest()

def interval(values):
    if not values:return None
    if len(set(values))==1:return [values[0],values[0]]
    rng=random.Random(20261019)
    samples=sorted(statistics.mean(rng.choices(values,k=len(values))) for _ in range(2000))
    return [samples[49],samples[1949]]

def wilson(success,total):
    if not total:return None
    z=1.959963984540054;p=success/total;den=1+z*z/total
    center=(p+z*z/(2*total))/den
    half=z*((p*(1-p)/total+z*z/(4*total*total))**.5)/den
    return [max(0,center-half),min(1,center+half)]

def oracle_metric(rows,label=None,response=None,target=None,den_label=None,den_response=None):
    denominator=[r for r in rows if (den_label is None or r['oracle_label']==den_label) and (den_response is None or r['policy_response']==den_response)]
    numerator=[r for r in denominator if (label is None or r['oracle_label']==label) and (response is None or r['policy_response']==response) and (target is None or r['target_achieved']==target)]
    return {'numerator':len(numerator),'denominator':len(denominator),'rate':len(numerator)/len(denominator) if denominator else None}

def oracle_summary(rows):
    return {
      'unsafe_release_rate':oracle_metric(rows,response='Released',den_label='Unsafe'),
      'false_safe_authorization_rate':oracle_metric(rows,label='Unsafe',den_response='Released'),
      'useful_completion_rate':oracle_metric(rows,target=True,den_label='SafeUseful'),
      'unnecessary_block_rate':oracle_metric(rows,response='Withheld',den_label='SafeUseful'),
      'safe_unnecessary_release_rate':oracle_metric(rows,response='Released',den_label='SafeUnnecessary')}

def summarize(rows,cases,freeze):
    result={'schema':freeze['schema'],'freeze':freeze,'decision_corpus':{},'closed_loop':{},'paired_confirmatory':{},
            'limits':['32 hand-authored contract fixtures are a finite conformance corpus, not a population sample.',
                      'Seed-block intervals are exploratory and conditional on the fixed seven-scenario mix.',
                      'No non-inferiority, general safety superiority or general 80 percent correctness claim.']}
    for policy in POLICIES:
        categories={}
        for category in ('Start','Run','Commit','Outcome'):
            selected=[c for c in cases['cases'] if c['category']==category]
            correct=sum(c['responses'][policy]==c['expected'] for c in selected)
            categories[category]={'correct':correct,'total':len(selected),'agreement':correct/len(selected)}
        result['decision_corpus'][policy]={'categories':categories,'equal_category_agreement':statistics.mean(c['agreement'] for c in categories.values()),'finite_corpus_only':True}
        selected=[r for r in rows if r['policy']==policy]
        blocks=defaultdict(list)
        for r in selected:
            if r['stage']=='confirmatory':blocks[r['seed']].append(r)
        unsafe_blocks=sum(any(r['score']['pressure_violation_ticks'] or r['score']['occupied_closing_motion_ticks'] for r in block) for block in blocks.values())
        result['closed_loop'][policy]={'runs':len(selected),'goal_achieved':sum(r['score']['objective_achieved'] for r in selected),
            'runs_with_pressure_violation':sum(r['score']['pressure_violation_ticks']>0 for r in selected),
            'false_completion_claims':sum(r['score']['false_completion_claims'] for r in selected),
            'unconfirmed_physical_targets':sum(r['score']['physical_targets_without_confirmation'] for r in selected),
            'released_commands':sum(len(r['score']['commands']) for r in selected),
            'unsafe_observed_command_windows':sum(not c['physical_safe_in_observed_window'] for r in selected for c in r['score']['commands']),
            'incomplete_command_windows':sum(not c['window_complete'] for r in selected for c in r['score']['commands']),
            'runs_with_containment_failure':sum(r['coverage']['contained_ticks']!=r['coverage']['total_ticks'] for r in selected),
            'confirmatory_unsafe_seed_blocks':unsafe_blocks,'confirmatory_seed_blocks':len(blocks),
            'unsafe_seed_block_rate_interval95':wilson(unsafe_blocks,len(blocks))}
        if freeze['schema']=='refinement-v2':
            candidates=[c for r in selected for c in r['candidate_evaluations']]
            result['closed_loop'][policy]['oracle']={'overall':oracle_summary(candidates),
                'by_role':{role:oracle_summary([c for c in candidates if c['role']==role]) for role in ('Primary','Recovery','Replanned')},
                'by_action':{action:oracle_summary([c for c in candidates if c['action']==action]) for action in ('AlarmOn','FanStart','FanStop','DoorClose','DoorOpen')}}
    lookup={(r['policy'],r['scenario'],r['seed']):r for r in rows if r['stage']=='confirmatory'}
    seeds=sorted({r['seed'] for r in rows if r['stage']=='confirmatory'})
    for comparator in ('B5','B4'):
        diffs=[];unsafe=[]
        for seed in seeds:
            pairs=[(lookup['B6',s,seed],lookup[comparator,s,seed]) for s in SCENARIOS]
            diffs.append(statistics.mean(int(a['score']['objective_achieved'])-int(b['score']['objective_achieved']) for a,b in pairs))
            unsafe.append(statistics.mean(int(a['score']['pressure_violation_ticks']>0)-int(b['score']['pressure_violation_ticks']>0) for a,b in pairs))
        result['paired_confirmatory']['B6-minus-'+comparator]={'seed_blocks':len(seeds),
            'goal_rate_difference':statistics.mean(diffs) if diffs else None,'goal_difference_bootstrap95':interval(diffs),
            'pressure_violation_rate_difference':statistics.mean(unsafe) if unsafe else None,'pressure_difference_bootstrap95':interval(unsafe)}
    return result

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path,required=True)
    parser.add_argument('--pilot-only',action='store_true')
    parser.add_argument('--oracle-v2',action='store_true',help='run the client-specified independent physical-oracle protocol')
    args=parser.parse_args();out=args.out.resolve()
    if out.exists() or not out.parent.is_dir():parser.error('choose a new output directory with an existing parent')
    subprocess.run(['cargo','build','--offline','--bin','model_demo','--bin','policy_cases'],cwd=ROOT,check=True)
    schema='refinement-v2' if args.oracle_v2 else 'refinement-v1'
    frozen=fingerprint(schema)
    freeze={'schema':schema,'source_protocol_sha256':frozen,'pilot_seeds':list(range(8)),
            'confirmatory_seeds':[] if args.pilot_only else list(range(8,24)),
            'scenarios':SCENARIOS,'policies':POLICIES,'primary_contrasts':['B6-minus-B5','B6-minus-B4'],'autonomous':False,'broader_recovery':False}
    out.mkdir();(out/'freeze.json').write_text(json.dumps(freeze,indent=2)+'\n')
    archive=out/'frozen-source';archive.mkdir()
    archived=hashlib.sha256()
    for path in source_paths(schema):
        relative=path.relative_to(ROOT);destination=archive/relative
        destination.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(path,destination)
        archived.update(str(relative).encode());archived.update(destination.read_bytes())
    if archived.hexdigest()!=frozen:raise RuntimeError('source changed while creating the frozen archive')
    (out/'frozen-source-manifest.json').write_text(json.dumps({'source_protocol_sha256':frozen,'files':[str(p.relative_to(ROOT)) for p in source_paths(schema)]},indent=2)+'\n')

    cases=json.loads(subprocess.check_output([str(ROOT/'target/debug/policy_cases')],text=True))
    (out/'decision-cases.json').write_text(json.dumps(cases,indent=2)+'\n')
    rows=[];seeds=freeze['pilot_seeds']+freeze['confirmatory_seeds']
    with (out/'runs.jsonl').open('w') as log:
        for seed in seeds:
            if fingerprint(schema)!=frozen:raise RuntimeError('source/protocol changed after freeze; use a new version/output')
            for scenario in SCENARIOS:
                for policy in POLICIES:
                    with tempfile.TemporaryDirectory(prefix='refinement-bench-') as temp:
                        run=Path(temp)/'run'
                        argv=[str(ROOT/'target/debug/model_demo'),'--scenario',scenario,'--seed',str(seed),'--policy',policy]
                        if args.oracle_v2:argv.append('--oracle-v2')
                        argv += ['--out',str(run)]
                        subprocess.run(argv,check=True,capture_output=True,text=True)
                        data=json.loads((run/'results.json').read_text())
                    keys=['scenario','seed','policy','policy_name','source_id','config_id','score','coverage','start_allowed','final_runtime_state']
                    if args.oracle_v2:keys.append('candidate_evaluations')
                    row={k:data[k] for k in keys}
                    row['stage']='pilot' if seed<8 else 'confirmatory'
                    row['replay_argv']=['model_demo','--scenario',scenario,'--seed',str(seed),'--policy',policy,'--out','NEW_DIRECTORY']
                    rows.append(row);log.write(json.dumps(row)+'\n');log.flush()
            print(f"{'Pilot' if seed<8 else 'Confirmatory'} seed {seed}: {len(rows)} runs saved",flush=True)
    if fingerprint(schema)!=frozen:raise RuntimeError('source/protocol changed during collection')
    result=summarize(rows,cases,freeze)
    (out/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
    lines=['# Refinement policy benchmark results','',f'Schema {schema}. Historical B0–B5 are unchanged.','',
           f"Collected {len(rows)} closed-loop runs and {len(cases['cases'])} fixed decision cases per policy.",'',
           '| Policy | Contract corpus agreement | Runs | Goals achieved | Runs with pressure violation | False completion |',
           '| --- | --- | --- | --- | --- | --- |']
    for policy in POLICIES:
        r=result['closed_loop'][policy];c=result['decision_corpus'][policy]
        lines.append(f"| {policy} | {c['equal_category_agreement']:.1%} | {r['runs']} | {r['goal_achieved']} | {r['runs_with_pressure_violation']} | {r['false_completion_claims']} |")
    lines+=['','The agreement column is finite contract-fixture conformance, not general decision accuracy. '
            'All physical and confirmation metrics are separate. See summary.json for per-category counts, '
            'paired seed-block contrasts, zero-event uncertainty and limitations. Replanning and recovery '
            'alternatives are excluded from these main comparisons.','',
            'The source/protocol was hashed before pilot collection and checked through confirmation. '
            'Use runs.jsonl replay arguments with the frozen source to reconstruct any full trace.']
    (out/'report.md').write_text('\n'.join(lines)+'\n')
    pointer='refinement-v2-latest.json' if args.oracle_v2 else 'refinement-latest.json'
    (ROOT/'output'/pointer).write_text(json.dumps({'summary_path':str(out/'summary.json')})+'\n')
    print(out/'report.md')
if __name__=='__main__':main()
