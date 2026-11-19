"""Repeat matched coarse/fine flow with conservative charge and uniform dt.

Start only after the existing GPU queue is terminal. Existing outputs are never
overwritten. The failed mixed-timestep fine run remains separate evidence.
"""

from datetime import datetime
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

root = Path("artifacts/hardware-research")
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--study", choices=["uniform", "temporal-retract", "precision-temporal-retract"], default="uniform")
parser.add_argument("--after-state", type=Path)
parser.add_argument("--after-pid", type=int)
parser.add_argument("--state", type=Path, help="Fresh state path for an explicitly reviewed unstarted study")
args = parser.parse_args()
if (args.after_state is None) != (args.after_pid is None):
    parser.error("Supply both predecessor state and controller PID")
state = args.state or root / {"uniform": "rooted-uniform-refinement-state.json",
                            "temporal-retract": "rooted-temporal-retract-state.json",
                            "precision-temporal-retract": "rooted-precision-temporal-retract-state.json"}[args.study]
env = {**os.environ, "TMPDIR": str(Path.cwd() / "artifacts/build-temp")}
binary = Path("target/debug/examples/conduction_flow_precision" if args.study == "precision-temporal-retract"
              else "target/debug/examples/conduction_flow")
jobs = []
specs = ([
    ("h010", "3", "1200", "300", ".25", "m6-rooted08-flow110-r25-chargefix-h010"),
    ("h0050", "1.5", "2400", "600", ".25", "m6-rooted08-flow110-r25-chargefix-h0050"),
] if args.study == "uniform" else [
    ("h010", "1.5", "2400", "600", ".25", "m6-rooted08-flow110-r25-chargefix-h010-dt1p5"),
    ("h010", "1.5", "2400", "600", ".5", "m6-rooted08-flow110-r50-chargefix-h010-dt1p5"),
])
if args.study == "precision-temporal-retract":
    specs = [
        ("h010", "3", "1200", "300", ".25", "m6-rooted08-flow110-r25-precisionv2-h010-dt3"),
        ("h010", "1.5", "2400", "600", ".25", "m6-rooted08-flow110-r25-precisionv2-h010-dt1p5"),
        ("h010", "1.5", "2400", "600", ".5", "m6-rooted08-flow110-r50-precisionv2-h010-dt1p5"),
    ]
for grid, dt, steps, every, retract, name in specs:
    output = root / name
    jobs.append(
        {
            "output": str(output),
            "command": [
                str(binary),
                str(root / ("cases/m6-rooted08-flow-compactgas-" + grid + ".mhd.json")),
                "110",
                "250",
                steps,
                str(output),
                "0",
                retract,
                dt,
                "0",
                every,
                dt,
                "25",
                "body",
            ],
        }
    )
if state.exists() or any(Path(j["output"]).exists() or Path(j["output"] + ".log").exists() for j in jobs):
    raise RuntimeError(
        "Existing uniform refinement state/output; inspect before repeating"
    )
binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
inputs = []
inputs.append(Path("target/debug/examples/adapter_inventory"))
for job in jobs:
    path = Path(job["command"][1])
    p = json.loads(path.read_text())
    geometry = path.parent / p["geometry"]
    inputs += [path, geometry] + [geometry.parent / b["uri"] for b in json.loads(geometry.read_text())["buffers"]]
hashes = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs}
adapter = None


def record(**values):
    temp = state.with_suffix(".new")
    temp.write_text(
        json.dumps(
            {
                "updated_at": datetime.now().astimezone().isoformat(),
                "binary_sha256": binary_hash,
                "binary_path": str(binary),
                "adapter_preflight": adapter,
                "input_sha256": hashes,
                "controller_pid": os.getpid(),
                "study": args.study,
                "jobs": jobs,
                "predecessor_state": str(args.after_state),
                "predecessor_pid": args.after_pid,
                **values,
            },
            indent=2,
        )
        + "\n"
    )
    temp.replace(state)


record(stage="waiting_for_predecessor" if args.after_state else "ready")
while args.after_state:
    previous = json.loads(args.after_state.read_text())
    try:
        os.kill(args.after_pid, 0)
        alive = True
    except ProcessLookupError:
        alive = False
    if previous["stage"] == "complete_needs_assessment" and not alive:
        break
    if not alive or previous["stage"] in ["failed", "weekly_stop", "predecessor_not_complete"]:
        record(stage="predecessor_not_complete", observed_state=previous)
        raise SystemExit(0)
    time.sleep(30)
lease = (root / "native-flow-sequence.lock").open("a")
fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
ps = subprocess.run(["ps", "-eo", "pid=,args="], capture_output=True, text=True, check=True)
if any(re.search(r"^\s*\d+\s+(?:\S*/)?target/(?:debug|release)/examples/(?:conduction_flow(?:_precision)?|conduction_probe|applied_probe)\s", line)
       for line in ps.stdout.splitlines()):
    record(stage="unexpected_active_native_job")
    raise RuntimeError("Native current-driven GPU job active")
adapter = json.loads(subprocess.check_output(["target/debug/examples/adapter_inventory"], text=True, env=env))
if "NVIDIA" not in adapter["default_headless_selected_name"]:
    record(stage="failed", error="Expected NVIDIA for this queue", adapter=adapter)
    raise RuntimeError("Unexpected default compute adapter")
record(stage="ready", adapter=adapter)
env["MHD_EXPECT_ADAPTER"] = adapter["default_headless_selected_name"]
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
    if any(hashlib.sha256(Path(path).read_bytes()).hexdigest() != value for path, value in hashes.items()):
        record(stage="failed", error="Frozen input changed")
        raise RuntimeError("Native input changed during paired study")
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
if args.study == "uniform":
    subprocess.run([sys.executable, "scripts/compare_rooted_flow_refinement.py"], check=True, env=env)
record(stage="complete_needs_assessment", completed_jobs=len(jobs))
