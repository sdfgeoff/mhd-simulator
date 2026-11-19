"""Run a frozen stationary DC plan serially after an explicitly named NVIDIA job."""
import argparse
from datetime import datetime
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import time

from run_unipolar_coil_screen import isolated_amd_process


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--plan", type=Path, required=True)
parser.add_argument("--after-state", type=Path, required=True)
parser.add_argument("--after-pid", type=int, required=True)
args = parser.parse_args()
root = Path("artifacts/hardware-research")
plan = json.loads(args.plan.read_text())
state = root / (args.plan.stem.removesuffix("-plan") + "-nvidia-state.json")
if state.exists() or any(Path(j["output"]).exists() or Path(j["output"] + ".log").exists() for j in plan["jobs"]):
    raise RuntimeError("Existing state/output; inspect before repeating")
binary = Path("target/debug/examples/conduction_probe")
inventory = Path("target/debug/examples/adapter_inventory")
paths = [args.plan, binary, inventory]
for job in plan["jobs"]:
    p = Path(job["project"])
    assert digest(p) == job["project_sha256"] and job["command"][0] == str(binary)
    project = json.loads(p.read_text())
    assert not project["run"]["fluid_enabled"] and not project["run"]["thermal_enabled"]
    assert all(not project["electromagnetics"][k] for k in ["wires", "magnets", "uniform"])
    geometry = p.parent / project["geometry"]
    paths += [p, geometry] + [geometry.parent / b["uri"] for b in json.loads(geometry.read_text())["buffers"]]
common = {"controller_pid": os.getpid(), "plan": str(args.plan),
          "predecessor_state": str(args.after_state), "predecessor_pid": args.after_pid,
          "input_and_binary_sha256": {str(p): digest(p) for p in paths},
          "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
          "binary_scope": "Previously built split-f32 stationary probe; recorded binary hash is authoritative, not current solver source version"}


def record(**values):
    temporary = state.with_suffix(".new")
    temporary.write_text(json.dumps({**common, "updated_at": datetime.now().astimezone().isoformat(), **values}, indent=2) + "\n")
    temporary.replace(state)


record(stage="waiting_for_predecessor")
try:
    while True:
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
    with (root / "native-flow-sequence.lock").open("a") as lease:
        fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
        for index, job in enumerate(plan["jobs"]):
            if any(digest(path) != value for path, value in common["input_and_binary_sha256"].items()):
                raise RuntimeError("Frozen input or binary changed")
            processes = subprocess.check_output(["ps", "-eo", "pid=,args="], text=True)
            pattern = r"^\s*\d+\s+(?:\S*/)?target/(?:debug|release)/(?:examples/(?:conduction_flow(?:_precision|_current_tolerance|_energy)?|conduction_probe|applied_probe|hardware_probe(?:_tail|_decay)?)\s|mhd-workbench\s+--run-check\s)"
            if any(re.search(pattern, line) and not isolated_amd_process(line, root / "radv-local-26.2.7/radeon-local.json") for line in processes.splitlines()):
                raise RuntimeError("Another NVIDIA solver is active")
            quota = subprocess.check_output([sys.executable, "/home/geoffrey/bin/weekly_remaining.py"], text=True)
            match = re.search(r"Weekly remaining:\s*(\d+)%", quota)
            if match is None:
                raise RuntimeError("Live quota unavailable")
            if int(match[1]) <= 40:
                record(stage="weekly_stop", weekly_remaining=int(match[1]))
                raise SystemExit(0)
            audit = json.loads(subprocess.check_output([str(inventory)], text=True))
            name = audit["default_headless_selected_name"]
            if "NVIDIA" not in name:
                raise RuntimeError("Unexpected physical adapter")
            log = Path(job["output"] + ".log")
            with log.open("x") as f:
                process = subprocess.Popen(job["command"], stdout=f, stderr=subprocess.STDOUT)
                record(stage="running", index=index, pid=process.pid, adapter=name, weekly_remaining=int(match[1]))
                code = process.wait()
            if code or name not in log.read_text():
                raise RuntimeError("Native probe failed or adapter provenance missing")
            report = json.loads((Path(job["output"]) / "report.json").read_text())
            for key, limit in plan["gates"].items():
                value = report[key]
                if not math.isfinite(value) or abs(value) > limit:
                    raise RuntimeError(f"Numerical gate failed: {key}={value}")
            expected_current = float(job["command"][2])
            if any(not math.isfinite(report[key]) for key in ["current_A", "return_current_A", "terminal_voltage_V", "joule_power_W"]):
                raise RuntimeError("Non-finite native electrical result")
            if report["current_A"] <= 0 or abs(report["current_A"] / expected_current - 1) > 1e-4 or abs(report["return_current_A"] / report["current_A"] + 1) > 2.1e-5:
                raise RuntimeError("Native port current differs from prepared command")
            (Path(job["output"]) / "adapter-provenance.json").write_text(json.dumps(audit, indent=2) + "\n")
            record(stage="completed_job", index=index)
    record(stage="complete_needs_assessment", completed_jobs=len(plan["jobs"]))
except Exception as error:
    record(stage="failed", error=str(error))
    raise
