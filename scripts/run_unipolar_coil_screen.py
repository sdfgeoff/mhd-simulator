"""Run the reviewed unipolar cold plan after a specific serial controller exits."""

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
    parser.add_argument("--after-state", type=Path, required=True)
    parser.add_argument("--after-pid", type=int, required=True)
    args = parser.parse_args()
    root = Path("artifacts/hardware-research")
    plan_path = root / "unipolar-coil-v1-plan.json"
    plan = json.loads(plan_path.read_text())
    state = root / "unipolar-coil-v1-state.json"
    if state.exists() or any(
        Path(j["output"]).exists() or Path(j["output"] + ".log").exists()
        for j in plan["jobs"]
    ):
        raise RuntimeError("Existing physical output/state; inspect before repeating")
    binary = Path("target/debug/examples/hardware_probe")
    hashes = {str(plan_path): sha(plan_path), str(binary): sha(binary)}
    for job in plan["jobs"]:
        if sha(job["project"]) != job["project_sha256"] or job["command"][0] != str(
            binary
        ):
            raise RuntimeError("Unexpected prepared job")
        project = Path(job["project"])
        p = json.loads(project.read_text())
        geometry = project.parent / p["geometry"]
        inputs = [project, geometry]
        inputs += [
            geometry.parent / b["uri"]
            for b in json.loads(geometry.read_text())["buffers"]
        ]
        hashes.update({str(path): sha(path) for path in inputs})
    common = {
        "controller_pid": os.getpid(),
        "input_and_binary_sha256": hashes,
        "predecessor_state": str(args.after_state),
        "predecessor_pid": args.after_pid,
        "plan": str(plan_path),
        "jobs": plan["jobs"],
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

    record(stage="waiting_for_predecessor")
    while True:
        previous = json.loads(args.after_state.read_text())
        try:
            os.kill(args.after_pid, 0)
            alive = True
        except ProcessLookupError:
            alive = False
        if previous["stage"] == "complete_needs_assessment" and not alive:
            break
        if (
            previous["stage"]
            in [
                "failed",
                "weekly_stop",
                "predecessor_not_complete",
                "invalid_completed_report",
            ]
            or not alive
        ):
            record(stage="predecessor_not_complete", observed_state=previous)
            return
        time.sleep(30)
    env = {**os.environ, "TMPDIR": str(Path.cwd() / "artifacts/build-temp")}
    with (root / "native-flow-sequence.lock").open("a") as lease:
        fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
        for index, job in enumerate(plan["jobs"]):
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
                raise RuntimeError("Another GPU job is active")
            if any(sha(path) != digest for path, digest in hashes.items()):
                raise RuntimeError("Prepared plan, binary or inputs changed")
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
            status = json.loads((Path(job["output"]) / "run-status.json").read_text())
            if not (
                status["status"] == "complete"
                and status["accepted_steps"] == 1200
                and abs(status["time_s"] - 600e-6) < 1e-10
            ):
                record(stage="invalid_completed_report", index=index, status=status)
                raise RuntimeError("Cold run did not complete requested interval")
            record(stage="completed_job", index=index)
        record(stage="complete_needs_assessment", completed_jobs=len(plan["jobs"]))


if __name__ == "__main__":
    main()
