"""Local closed-loop experiment service, separate from the old planner lock."""
import json
from pathlib import Path
import subprocess
import threading
import uuid

ROOT=Path(__file__).resolve().parents[1]
SCENARIOS=('normal','short-gap','recovery','uncertain','lost-feedback','delayed-command','delayed-reading','hazard-recovery','replanning')
class ModelService:
    def __init__(self):self.slots=threading.BoundedSemaphore(2)
    def run(self,request):
        if not isinstance(request,dict) or set(request)-{'scenario','policy','seed','autonomous','broader_recovery'}:
            raise ValueError('Invalid experiment fields')
        scenario=request.get('scenario','normal');policy=request.get('policy','B6');seed=request.get('seed',42)
        if scenario not in SCENARIOS or policy not in tuple('B'+str(i) for i in range(7)):
            raise ValueError('Unknown scenario or policy')
        if type(seed) is not int or not 0<=seed<=2**64-1:raise ValueError('Seed must be an unsigned 64-bit integer')
        if any(type(request.get(k,False)) is not bool for k in ('autonomous','broader_recovery')):raise ValueError('Planner and recovery options must be boolean')
        if not self.slots.acquire(blocking=False):raise RuntimeError('Two experiments are already running; try again shortly')
        try:
            run_id=str(uuid.uuid4());parent=ROOT/'output/model-dashboard';parent.mkdir(parents=True,exist_ok=True)
            destination=parent/run_id
            args=[str(ROOT/'target/debug/model_demo'),'--scenario',scenario,'--seed',str(seed),'--policy',policy,'--out',str(destination)]
            if request.get('autonomous'):args.append('--autonomous')
            if request.get('broader_recovery'):args.append('--broader-recovery')
            result=subprocess.run(args,cwd=ROOT,capture_output=True,text=True,timeout=30)
            if result.returncode:raise RuntimeError('Experiment failed: '+result.stderr[-500:])
            data=json.loads((destination/'results.json').read_text());data['run_id']=run_id
            return data
        finally:self.slots.release()
    def benchmark(self):
        pointer=ROOT/'output/refinement-latest.json'
        if not pointer.exists():return {'available':False,'message':'The frozen benchmark campaign has not completed yet.'}
        location=json.loads(pointer.read_text());path=Path(location['summary_path']).resolve()
        if not path.is_relative_to(ROOT/'output'):raise RuntimeError('Invalid benchmark output location')
        data=json.loads(path.read_text());return {'available':True,'summary':data}
