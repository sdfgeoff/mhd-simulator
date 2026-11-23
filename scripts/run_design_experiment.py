"""Run one frozen design experiment on an explicitly selected isolated adapter."""
from datetime import datetime
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import shutil
import sys

ROOT=Path('artifacts/hardware-research')
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--plan',type=Path,required=True)
parser.add_argument('--state',type=Path,required=True)
args=parser.parse_args()
STATE=args.state
PLAN=args.plan
if STATE.exists():raise RuntimeError('Existing state: inspect it; never silently repeat')
plan=json.loads(PLAN.read_text())
if len(plan['jobs'])!=1:raise RuntimeError('Expected exactly one long experiment per design')
lease=(ROOT/'native-flow-sequence.lock').open('a')
fcntl.flock(lease,fcntl.LOCK_EX|fcntl.LOCK_NB)
env={**os.environ,'TMPDIR':str(Path.cwd()/'artifacts/build-temp'),
     'VK_DRIVER_FILES':plan['driver_manifest']}
frozen=[Path(__file__),PLAN,Path(plan['binary']),Path('target/debug/examples/adapter_inventory'),
        Path(env['VK_DRIVER_FILES'])]
if plan.get('archive_completed_fields'):frozen.append(Path('scripts/archive_native_field.py'))
manifest=json.loads(Path(env['VK_DRIVER_FILES']).read_text())
lib=Path(manifest['ICD']['library_path'])
if lib.is_absolute():frozen.append(lib)
for job in plan['jobs']:
    project=Path(job['command'][1]);p=json.loads(project.read_text());g=Path(p['geometry'])
    frozen.extend([project,g,*[g.parent/b['uri'] for b in json.loads(g.read_text())['buffers']]])
    if Path(job['output']).exists() or Path(job['output']+'.log').exists():
        raise RuntimeError('Existing output; refuse overwrite')
hashes={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in frozen}
if hashes[plan['binary']]!=plan['expected_binary_sha256']:raise RuntimeError('Baseline binary differs')
def record(**values):
    d={'updated_at':datetime.now().astimezone().isoformat(),'controller_pid':os.getpid(),
       'authorization_reply':plan['authorization_reply'],'plan':str(PLAN),'input_and_binary_sha256':hashes,**values}
    t=STATE.with_suffix('.new');t.write_text(json.dumps(d,indent=2)+'\n');t.replace(STATE)
record(stage='preflight')
try:
    # Refuse compute overlap. Read /proc so shell text is not mistaken for a solver.
    for proc in Path('/proc').iterdir():
        if not proc.name.isdigit():continue
        try:arg=proc.joinpath('cmdline').read_bytes().split(b'\0')[0].decode()
        except (FileNotFoundError,PermissionError,ProcessLookupError):continue
        if re.fullmatch(r'(?:.*/)?(?:conduction_flow[^/]*|(?:amd_)?hardware_probe[^/]*|conduction_probe|applied_probe|thermal_conductance)',arg):
            raise RuntimeError(f'Existing native compute PID{proc.name}: {arg}')
    inventory=json.loads(subprocess.check_output(['target/debug/examples/adapter_inventory'],env=env,text=True))
    if len(inventory['vulkan_adapters'])!=1 or inventory['vulkan_adapters'][0]['vendor']!=plan['expected_adapter_vendor']:
        raise RuntimeError('Expected the isolated adapter selected in the frozen plan')
    if inventory['default_headless_selected_name']!=plan['expected_adapter_name']:raise RuntimeError('Unexpected adapter name')
    limits=inventory['vulkan_adapters'][0]
    if plan['largest_field_buffer_bytes']>min(limits['max_buffer_size'],limits['max_storage_buffer_binding_size']):raise RuntimeError('Planned field exceeds adapter buffer limits')
    env['MHD_EXPECT_ADAPTER']=inventory['default_headless_selected_name']
    for index,job in enumerate(plan['jobs']):
        quota=subprocess.check_output([sys.executable,'/home/geoffrey/bin/weekly_remaining.py'],text=True)
        m=re.search(r'Weekly remaining:\s*(\d+)%',quota)
        if not m:raise RuntimeError('Live weekly check unavailable')
        if int(m.group(1))<=40:
            record(stage='weekly_stop',index=index,weekly_remaining=int(m.group(1)));sys.exit(0)
        for path,digest in hashes.items():
            if hashlib.sha256(Path(path).read_bytes()).hexdigest()!=digest:raise RuntimeError('Frozen input changed: '+path)
        if shutil.disk_usage(ROOT).free<plan['minimum_free_disk_bytes']:raise RuntimeError('Insufficient disk space for the full long experiment plus reserve')
        with Path(job['output']+'.log').open('x') as log:
            process=subprocess.Popen(job['command'],env=env,stdout=log,stderr=subprocess.STDOUT)
            record(stage='running',index=index,pid=process.pid,job=job,adapter=inventory,weekly_remaining=int(m.group(1)))
            code=process.wait()
        if code:raise RuntimeError(f'Native case {index} failed with exit{code}; preserve output')
        status=json.loads((Path(job['output'])/'run-status.json').read_text())
        if status['status']!='complete' or status['accepted_steps']!=job.get('expected_steps',2400) or abs(status['time_s']-job.get('expected_time_s',.0036))>1e-12:
            raise RuntimeError('Native result incomplete')
        if plan.get('archive_completed_fields'):
            fields=sorted(Path(job['output']).glob('frame-*/fluid.bin'))
            subprocess.run([sys.executable,'scripts/archive_native_field.py',*[str(f) for f in fields]],check=True,env=env)
        record(stage='analysing_saved_output',index=index)
        with Path(job['output']+'-report.log').open('x') as log:
            subprocess.run(['uv','run','--offline','--with','scipy','--with','numpy','--with','matplotlib',
                            'python','scripts/hardware_flow_report.py',job['output'],'--plot','--pool-view',*job.get('analysis_args',[])],
                           check=True,env=env,stdout=log,stderr=subprocess.STDOUT)
    record(stage='complete_needs_assessment',completed_jobs=len(plan['jobs']))
except Exception as error:
    record(stage='failed',error=str(error));raise
