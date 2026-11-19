"""Serial matched slit controls; wait for the explicitly named predecessor.

The conditioning interval is a comparison control, not a claim of equilibrium.
Never overwrite or resume an existing physical output.
"""

import argparse
from datetime import datetime
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--after-state", type=Path)
    parser.add_argument("--after-pid", type=int)
    parser.add_argument("--state", type=Path)
    args = parser.parse_args()
    if (args.after_state is None) != (args.after_pid is None):
        parser.error("Supply both predecessor state and controller PID")
    root = Path("artifacts/hardware-research")
    state = args.state or root / "split-slit-conditioned-pair-state.json"
    project = root / "cases/split-slit-steel-flow-h010.mhd.json"
    binary = Path("target/debug/examples/conduction_flow")
    outputs = [root / f"split-slit-steel-conditioned-{a}A-xends-h010" for a in [0, 40]]
    if state.exists() or any(
        p.exists() or Path(str(p) + ".log").exists() for p in outputs
    ):
        raise RuntimeError("Existing state/output: inspect, do not repeat")
    p = json.loads(project.read_text())
    geometry = Path(p["geometry"])
    inputs = [project, geometry]
    gltf = json.loads(geometry.read_text())
    inputs.extend(geometry.parent / b["uri"] for b in gltf["buffers"])
    hashes = {str(path): sha(path) for path in inputs}
    binary_hash = sha(binary)
    jobs = []
    for peak, output in zip([0, 40], outputs):
        jobs.append(
            {
                "peak_A": peak,
                "output": str(output),
                "command": [
                    str(binary),
                    str(project),
                    str(peak),
                    "1000",
                    "3334",
                    str(output),
                    "0",
                    "0.25",
                    "3",
                    "0",
                    "167",
                    "3",
                    "25",
                    "x-ends",
                    "5001",
                ],
            }
        )
    common = {
        "controller_pid": os.getpid(),
        "binary_sha256": binary_hash,
        "input_sha256": hashes,
        "jobs": jobs,
        "predecessor": str(args.after_state),
        "predecessor_controller_pid": args.after_pid,
        "parameters": {
            "conditioning_s": 0.005001,
            "total_s": 0.010002,
            "dt_s": 3e-6,
            "steps": 3334,
            "positive_s": 0.001,
            "retract_fraction": 0.25,
            "terminal": "x-ends",
        },
        "limits": [
            "100um cells give only two cells across the0.2mm gap.",
            "Conditioning is not demonstrated equilibrium. Compare identical prefixes and zero-current evolution.",
            "40A is an exploratory waveform, not a predicted threshold or qualified operating point.",
            "No thermal evolution, calibrated contact angle, oxide, feed replenishment or repetitive-rate claim.",
        ],
    }

    def record(**values):
        temp = state.with_suffix(".new")
        temp.write_text(
            json.dumps(
                {
                    **common,
                    "updated_at": datetime.now().astimezone().isoformat(),
                    **values,
                },
                indent=2,
            )
            + "\n"
        )
        temp.replace(state)

    if args.after_state:
        record(stage="waiting_for_predecessor")
        while True:
            predecessor = json.loads(args.after_state.read_text())
            try:
                os.kill(args.after_pid, 0)
                alive = True
            except ProcessLookupError:
                alive = False
            if predecessor["stage"] == "complete_needs_assessment":
                if not alive:
                    break
            elif predecessor["stage"] in ["failed", "weekly_stop"] or not alive:
                record(stage="predecessor_not_complete", predecessor_state=predecessor)
                return
            time.sleep(30)

    env = {**os.environ, "TMPDIR": str(Path.cwd() / "artifacts/build-temp")}
    with (root / "native-flow-sequence.lock").open("a") as lease:
        fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
        for index, job in enumerate(jobs):
            processes = subprocess.run(
                ["ps", "-eo", "pid=,args="], capture_output=True, text=True, check=True
            ).stdout
            if any(
                re.search(
                    r"^\s*\d+\s+(?:\S*/)?target/(?:debug|release)/(?:examples/(?:conduction_flow|conduction_probe|applied_probe|hardware_probe)\s|mhd-workbench\s+--run-check\s)",
                    line,
                )
                for line in processes.splitlines()
            ):
                raise RuntimeError("Another native GPU job is active")
            quota = subprocess.run(
                [sys.executable, "/home/geoffrey/bin/weekly_remaining.py"],
                capture_output=True,
                text=True,
                check=True,
            ).stdout
            match = re.search(r"Weekly remaining:\s*(\d+)%", quota)
            if match is None:
                raise RuntimeError("Live quota unavailable")
            if int(match[1]) <= 40:
                record(stage="weekly_stop", weekly_remaining=int(match[1]))
                return
            if sha(binary) != binary_hash or any(
                sha(path) != digest for path, digest in hashes.items()
            ):
                raise RuntimeError("Prepared binary or inputs changed")
            with Path(job["output"] + ".log").open("x") as log:
                process = subprocess.Popen(
                    job["command"], stdout=log, stderr=subprocess.STDOUT, env=env
                )
                record(
                    stage="running",
                    index=index,
                    pid=process.pid,
                    weekly_remaining=int(match[1]),
                )
                code = process.wait()
            if code:
                record(stage="failed", index=index, returncode=code)
                return
            report = json.loads((Path(job["output"]) / "run-status.json").read_text())
            if not (
                report["status"] == "complete"
                and report["accepted_steps"] == report["requested_steps"] == 3334
                and abs(report["time_s"] - 0.010002) < 1e-9
                and abs(report["pulse_start_s"] - 0.005001) < 1e-12
                and report["terminal_boundary"] == "x-ends"
                and report["peak_A"] == job["peak_A"]
                and report["max_charge_boundary_current_defect"] <= 1e-5
            ):
                record(stage="invalid_completed_report", index=index, report=report)
                raise RuntimeError("Native report failed completion/physics gates")
            with Path(job["output"] + "-report.log").open("x") as log:
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
                    check=True,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    env=env,
                )
            record(stage="reported", index=index)
        record(stage="complete_needs_assessment", completed_jobs=len(jobs))


if __name__ == "__main__":
    main()
