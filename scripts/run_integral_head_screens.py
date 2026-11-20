"""Frozen, sequential integral-head screens authorized by board842/838/824."""
import argparse,fcntl,hashlib,json,os,re,subprocess,sys
from datetime import datetime
from pathlib import Path
root=Path('artifacts/hardware-research')
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--plan',type=Path,default=Path('hardware/integral-head-review/electrical-plan-v1.json'));parser.add_argument('--state',type=Path,default=root/'integral-head-electrical-v1-state.json');args=parser.parse_args();state=args.state
assert not state.exists(),'Inspect existing state; do not repeat'
lease=(root/'native-flow-sequence.lock').open('a');fcntl.flock(lease,fcntl.LOCK_EX|fcntl.LOCK_NB)
env={**os.environ,'VK_DRIVER_FILES':'/usr/share/vulkan/icd.d/nvidia_icd.json','TMPDIR':str(Path.cwd()/'artifacts/build-temp')}
for proc in Path('/proc').iterdir():
    if not proc.name.isdigit():continue
    try:arg=Path(proc/'cmdline').read_bytes().split(b'\0')[0].decode()
    except (FileNotFoundError,PermissionError,ProcessLookupError):continue
    if re.fullmatch(r'(?:.*/)?(?:conduction_flow(?:_precision|_current_tolerance|_energy)?|hardware_probe(?:_tail|_decay)?|conduction_probe|applied_probe|thermal_conductance)',arg):raise RuntimeError('Existing native compute '+proc.name)
plan=json.loads(args.plan.read_text());assert hashlib.sha256(Path(plan['binary']).read_bytes()).hexdigest()==plan['binary_sha256'],'Binary differs from plan'
paths=[Path(__file__),args.plan,Path(plan['binary']),Path('target/debug/examples/adapter_inventory'),Path(env['VK_DRIVER_FILES'])]
for path,digest in plan.get('expected_binary_hashes',{}).items():
    assert hashlib.sha256(Path(path).read_bytes()).hexdigest()==digest,'Binary differs from plan: '+path
for job in plan['jobs']:
    paths.append(Path(job.get('command',[plan['binary']])[0]))
    paths.extend(Path(p) for p in job.get('extra_inputs',[]))
    p=Path(job['project']);g=Path(json.loads(p.read_text())['geometry']);paths.extend([p,g,*[g.parent/b['uri'] for b in json.loads(g.read_text())['buffers']]])
    assert not Path(job['output']).exists() and not Path(job['output']+'.log').exists(),'Existing result'
hashes={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
def record(**values):
    p=state.with_suffix('.new');p.write_text(json.dumps({'time':datetime.now().astimezone().isoformat(),'controller_pid':os.getpid(),'authorization_replies':plan.get('authorization_replies',[824,838,842]),'hashes':hashes,'plan':str(args.plan),**values},indent=2)+'\n');p.replace(state)
record(stage='preflight')
try:
    inv=json.loads(subprocess.check_output(['target/debug/examples/adapter_inventory'],env=env,text=True));assert len(inv['vulkan_adapters'])==1 and inv['vulkan_adapters'][0]['vendor']==0x10de
    env['MHD_EXPECT_ADAPTER']=inv['default_headless_selected_name']
    for i,job in enumerate(plan['jobs']):
        quota=subprocess.check_output([sys.executable,'/home/geoffrey/bin/weekly_remaining.py'],text=True);m=re.search(r'Weekly remaining:\s*(\d+)%',quota);assert m,'Quota unavailable'
        if int(m.group(1))<=40:record(stage='weekly_stop',index=i);sys.exit(0)
        for p,h in hashes.items():assert hashlib.sha256(Path(p).read_bytes()).hexdigest()==h,'Frozen input changed: '+p
        command=job.get('command',[plan['binary'],job['project'],'100',job['output'],'0','x-ends','1e-6'])
        with Path(job['output']+'.log').open('x') as log:
            native=subprocess.Popen(command,env=env,stdout=log,stderr=subprocess.STDOUT)
            record(stage='running',index=i,pid=native.pid,job=job,command=command,adapter=inv,weekly_remaining=int(m.group(1)))
            code=native.wait()
        if code:raise RuntimeError(f'Native exit {code}; preserve failure')
        r=json.loads((Path(job['output'])/'report.json').read_text())
        assert r['true_relative_residual']<=1.1*r.get('requested_relative_tolerance',1e-6)
        if job.get('screen',plan.get('screen','electrical'))=='thermal':
            assert r['conductance_W_K']>0 and r['return_relative_error']<=1e-5 and abs(r['energy_identity_relative_error'])<=1e-4
        else:
            assert abs(r['current_A']/100-1)<1e-4 and r['boundary_current_defect']<=1e-5 and abs(r['power_relative_defect'])<=1e-4
        record(stage='completed_case',index=i,job=job)
    record(stage='complete_needs_assessment',completed_jobs=len(plan['jobs']))
except Exception as e:record(stage='failed',error=str(e));raise
