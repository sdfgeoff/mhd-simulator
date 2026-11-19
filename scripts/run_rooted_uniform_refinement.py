"""Repeat matched coarse/fine flow with conservative charge and uniform dt.

Start only after the existing GPU queue is terminal. Existing outputs are never
overwritten. The failed mixed-timestep fine run remains separate evidence.
"""

from datetime import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

root = Path("artifacts/hardware-research")
state = root / "rooted-uniform-refinement-state.json"
env = {**os.environ, "TMPDIR": str(Path.cwd() / "artifacts/build-temp")}
jobs = []
for grid, dt, steps, every in [
    ("h010", "3", "1200", "300"),
    ("h0050", "1.5", "2400", "600"),
]:
    output = root / ("m6-rooted08-flow110-r25-chargefix-" + grid)
    jobs.append(
        {
            "output": str(output),
            "command": [
                "target/debug/examples/conduction_flow",
                str(root / ("cases/m6-rooted08-flow-compactgas-" + grid + ".mhd.json")),
                "110",
                "250",
                steps,
                str(output),
                "0",
                ".25",
                dt,
                "0",
                every,
                dt,
                "25",
                "body",
            ],
        }
    )
if state.exists() or any(Path(j["output"]).exists() for j in jobs):
    raise RuntimeError(
        "Existing uniform refinement state/output; inspect before repeating"
    )
ps = subprocess.run(
    ["ps", "-eo", "pid=,args="], capture_output=True, text=True, check=True
)
if any(
    re.search(
        r"^\s*\d+\s+(?:\S*/)?target/(?:debug|release)/examples/(?:conduction_flow|conduction_probe|applied_probe)\s",
        line,
    )
    for line in ps.stdout.splitlines()
):
    raise RuntimeError("Native GPU job active; wait for its queue to finish")
binary = Path("target/debug/examples/conduction_flow")
binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()


def record(**values):
    temp = state.with_suffix(".new")
    temp.write_text(
        json.dumps(
            {
                "updated_at": datetime.now().astimezone().isoformat(),
                "binary_sha256": binary_hash,
                **values,
            },
            indent=2,
        )
        + "\n"
    )
    temp.replace(state)


for index, job in enumerate(jobs):
    quota = subprocess.run(
        [sys.executable, "/home/geoffrey/bin/weekly_remaining.py"],
        capture_output=True,
        text=True,
    )
    match = re.search(r"Weekly remaining:\s*(\d+)%", quota.stdout)
    if quota.returncode or not match:
        raise RuntimeError("Live budget check failed")
    if int(match.group(1)) <= 40:
        record(stage="weekly_stop", weekly_remaining=int(match.group(1)))
        raise SystemExit(0)
    if hashlib.sha256(binary.read_bytes()).hexdigest() != binary_hash:
        raise RuntimeError("Native binary changed during paired refinement")
    with Path(job["output"] + ".log").open("x") as log:
        process = subprocess.Popen(
            job["command"], stdout=log, stderr=subprocess.STDOUT, env=env
        )
        record(
            stage="running",
            index=index,
            pid=process.pid,
            job=job,
            weekly_remaining=int(match.group(1)),
        )
        code = process.wait()
    if code:
        record(stage="failed", job=job, returncode=code)
        raise RuntimeError("Inspect " + job["output"] + ".log")
    subprocess.run(
        [
            "uv",
            "run",
            "--offline",
            "--with",
            "scipy",
            "--with",
            "numpy",
            "--with",
            "matplotlib",
            "python",
            "scripts/hardware_flow_report.py",
            job["output"],
            "--plot",
            "--pool-view",
        ],
        env=env,
        check=True,
        stdout=(root / (Path(job["output"]).name + "-report.log")).open("x"),
    )
    record(stage="reported", index=index, job=job)
subprocess.run(
    [sys.executable, "scripts/compare_rooted_flow_refinement.py"], check=True, env=env
)
record(stage="complete_needs_assessment", completed_jobs=len(jobs))
