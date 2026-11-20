"""Read-only, one-minute resource snapshots while the approved pair runs."""
from datetime import datetime
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import time

root=Path('artifacts/hardware-research')
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--state',type=Path,default=root/'plain-bore-v2-state.json')
parser.add_argument('--log',type=Path,default=root/'plain-bore-v2-resources.jsonl')
args=parser.parse_args()
state=args.state
log=args.log
while True:
    row={'time':datetime.now().astimezone().isoformat()}
    try:
        s=json.loads(state.read_text());row.update(stage=s['stage'],index=s.get('index'),native_pid=s.get('pid'))
        row['disk_free_bytes']=shutil.disk_usage(root).free
        mem={line.split(':')[0]:line.split(':')[1].strip() for line in Path('/proc/meminfo').read_text().splitlines()}
        row['host_memory']={k:mem[k] for k in ['MemTotal','MemAvailable','SwapTotal','SwapFree']}
        p=Path(f'/proc/{s.get("pid")}');row['native_alive']=p.exists()
        row['controller_alive']=Path(f'/proc/{s["controller_pid"]}').exists()
        if p.exists():
            row['process_memory']=[x for x in (p/'status').read_text().splitlines() if x.startswith(('VmRSS:','VmHWM:','VmSwap:'))]
            cg=(p/'cgroup').read_text().strip().split(':')[-1]
            cgroup=Path('/sys/fs/cgroup')/cg.lstrip('/')
            for name in ['memory.events','memory.pressure','memory.current','memory.max']:
                path=cgroup/name
                if path.exists():row[name]=path.read_text().strip()
        row['gpu']=subprocess.check_output(['nvidia-smi','--query-gpu=temperature.gpu,memory.used,memory.total,utilization.gpu,power.draw','--format=csv,noheader,nounits'],text=True,timeout=10).strip()
        row['gpu_columns']=['temperature_C','memory_used_MiB','memory_total_MiB','utilization_percent','power_W']
    except Exception as error:row['sampling_error']=str(error)
    with log.open('a') as out:out.write(json.dumps(row)+'\n')
    if row.get('stage') in ['complete_needs_assessment','failed','weekly_stop'] or row.get('controller_alive') is False:break
    time.sleep(60)
