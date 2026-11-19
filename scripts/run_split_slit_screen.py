"""Serial cold GPU comparisons, deferred until the existing flow job exits.

Use --prepare-only to write and inspect the plan without starting GPU work.
No automatic resumption of existing output directories.
"""

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
from datetime import datetime

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--prepare-only", action="store_true")
parser.add_argument("--tag", default="chargefix-v1")
parser.add_argument("--tolerance", type=float, default=1e-6)
args = parser.parse_args()
if not re.fullmatch(r"[a-zA-Z0-9-]+", args.tag) or not 0 < args.tolerance < 1:
    parser.error("Use an alphanumeric/hyphen tag and a tolerance between0and1")
root = Path("artifacts/hardware-research")
source = Path("hardware/designs/split-slit-review")
plan_path = root / f"split-slit-{args.tag}-plan.json"
state_path = root / f"split-slit-{args.tag}-state.json"
if state_path.exists():
    raise RuntimeError("Existing queue state; inspect it and use a new tag")
env = {**os.environ, "TMPDIR": str(Path.cwd() / "artifacts/build-temp")}
jobs = []


def record(**values):
    temporary = state_path.with_suffix(".new")
    temporary.write_text(
        json.dumps(
            {"updated_at": datetime.now().astimezone().isoformat(), **values}, indent=2
        )
        + "\n"
    )
    temporary.replace(state_path)


def charge(label, project, suffix):
    output = root / ("split-slit-" + args.tag + "-" + label + "-" + suffix)
    jobs.append(
        {
            "label": label,
            "kind": suffix,
            "output": str(output),
            "command": [
                "target/debug/examples/conduction_probe",
                str(project),
                "30",
                str(output),
                "0",
                "x-ends",
                str(args.tolerance),
            ],
        }
    )


for label in ["alumina", "steel", "steel_touch", "alumina_touch"]:
    # Full anchor width at coarse resolution; narrow control isolates the
    # exterior-domain change from subsequent spatial refinement.
    wide_path = source / (label + "-charge-h010.mhd.json")
    charge(label, wide_path, "wide-h010")
    narrow = copy.deepcopy(json.loads(wide_path.read_text()))
    narrow["geometry"] = str((source / narrow["geometry"]).resolve())
    narrow["domain"].update(min_mm=[-12.5, -3.2, -2.2], max_mm=[12.5, 3.2, 10.5])
    narrow_path = (
        root
        / "cases"
        / ("split-slit-" + args.tag + "-" + label + "-narrow-h010.mhd.json")
    )
    narrow_path.write_text(json.dumps(narrow, indent=2) + "\n")
    charge(label, narrow_path, "narrow-h010")

observer_output = root / f"split-slit-{args.tag}-field-h0050"
jobs.append(
    {
        "label": "common",
        "kind": "field-h0050",
        "output": str(observer_output),
        "command": [
            "target/debug/examples/applied_probe",
            str(source / "alumina-observer.mhd.json"),
            str(observer_output),
        ],
    }
)
for label in ["alumina", "steel", "steel_touch", "alumina_touch"]:
    charge(label, source / (label + "-charge.mhd.json"), "narrow-h0050")

plan = {
    "scope": "Cold stationary current/bypass and imposed-PM-field comparisons only; no slit ejection, thermal, material-lifetime or seal qualification.",
    "current_A": 30,
    "contact_resistivity_ohm_m2": 0,
    "terminal_boundary": "x-ends",
    "relative_tolerance": args.tolerance,
    "boundary_current_defect_limit": 1e-5,
    "tag": args.tag,
    "solver_sources_sha256": {
        str(p): hashlib.sha256(p.read_bytes()).hexdigest()
        for p in [
            Path("examples/conduction/gpu.rs"),
            Path("examples/conduction/solver.wgsl"),
            Path("examples/conduction_probe.rs"),
        ]
    },
    "probe_binary_sha256": hashlib.sha256(
        Path("target/debug/examples/conduction_probe").read_bytes()
    ).hexdigest(),
    "jobs": jobs,
    "controls": [
        "Compare full/narrow anchor width at the same100um spacing before interpreting50um results.",
        "Compare conducting/insulating chamber and deliberate electrode-to-holder bridges.",
        "Field observer is common: all cases have identical PM source geometry and identical liquid/Cu occupancy in the observation region; unit permeability, no field reaction.",
        "Fixed conductivity assumptions do not constitute a hot aluminium/copper comparison.",
    ],
}
if plan_path.exists() and json.loads(plan_path.read_text()) != plan:
    raise RuntimeError("Existing plan differs; use a fresh tag")
plan_path.write_text(json.dumps(plan, indent=2) + "\n")
print(plan_path, flush=True)
if args.prepare_only:
    raise SystemExit(0)
for job in jobs:
    if Path(job["output"]).exists():
        raise RuntimeError("Existing output: " + job["output"])


def gpu_jobs():
    result = subprocess.run(
        ["ps", "-eo", "pid=,args="], capture_output=True, text=True, check=True
    )
    # Match actual native binaries, not this queue or the command inspecting it.
    return [
        line.strip()
        for line in result.stdout.splitlines()
        if re.search(
            r"^\s*\d+\s+(?:\S*/)?target/(?:debug|release)/examples/(?:conduction_flow|conduction_probe|applied_probe)\s",
            line,
        )
    ]


record(stage="waiting_for_gpu", pending_jobs=len(jobs))
while gpu_jobs():
    time.sleep(30)
for index, job in enumerate(jobs):
    if gpu_jobs():
        record(stage="stopped_external_gpu_job", job=job)
        raise RuntimeError("Another GPU job appeared; inspect before continuing")
    quota = subprocess.run(
        [sys.executable, "/home/geoffrey/bin/weekly_remaining.py"],
        capture_output=True,
        text=True,
    )
    match = re.search(r"Weekly remaining:\s*(\d+)%", quota.stdout)
    if quota.returncode or not match:
        record(stage="stopped_quota_unavailable")
        raise RuntimeError("Live quota check failed")
    if int(match.group(1)) <= 40:
        record(stage="weekly_stop", weekly_remaining=int(match.group(1)))
        raise SystemExit(0)
    log_path = Path(job["output"] + ".log")
    with log_path.open("x") as log:
        process = subprocess.Popen(
            job["command"], stdout=log, stderr=subprocess.STDOUT, env=env
        )
        record(
            stage="running",
            pid=process.pid,
            index=index,
            job=job,
            weekly_remaining=int(match.group(1)),
        )
        code = process.wait()
    if code:
        record(stage="failed", returncode=code, job=job)
        raise RuntimeError("Inspect " + str(log_path))
    report = json.loads((Path(job["output"]) / "report.json").read_text())
    if job["kind"] != "field-h0050":
        if not (
            abs(report["power_relative_defect"]) < 1e-4
            and report["true_relative_residual"] <= args.tolerance
            and report["boundary_current_defect"] <= 1e-5
            and abs(report["return_current_A"] / report["current_A"] + 1) < 2.1e-5
        ):
            record(stage="failed_numerical_gate", job=job)
            raise RuntimeError("Numerical gate failed")
    print("Completed", job["output"], flush=True)
record(stage="complete_needs_assessment", completed_jobs=len(jobs))
