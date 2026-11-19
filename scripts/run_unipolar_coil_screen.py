"""Run a reviewed unipolar plan, with one solver per physical adapter."""

import argparse
import copy
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
    parser.add_argument("--plan", type=Path)
    parser.add_argument("--adapter", choices=["default", "amd"], default="default")
    args = parser.parse_args()
    if (args.after_state is None) != (args.after_pid is None):
        parser.error("Supply both predecessor state and controller PID")
    root = Path("artifacts/hardware-research")
    plan_path = args.plan or root / "unipolar-coil-v2-plan.json"
    plan = json.loads(plan_path.read_text())
    state = args.state or root / (
        plan_path.stem.removesuffix("-plan") + "-" + args.adapter + "-state.json"
    )
    if state.exists() or any(
        Path(j["output"]).exists() or Path(j["output"] + ".log").exists()
        for j in plan["jobs"]
    ):
        raise RuntimeError("Existing physical output/state; inspect before repeating")
    original_binary = Path("target/debug/examples/hardware_probe")
    binary = (
        Path("target/debug/examples/amd_hardware_probe")
        if args.adapter == "amd"
        else original_binary
    )
    hashes = {str(plan_path): sha(plan_path), str(binary): sha(binary)}
    for job in plan["jobs"]:
        if sha(job["project"]) != job["project_sha256"] or job["command"][0] != str(
            original_binary
        ):
            raise RuntimeError("Unexpected prepared job")
        if int(job["command"][2]) != job.get("requested_steps", 1200):
            raise RuntimeError("Prepared step count differs from the command")
        project = Path(job["project"])
        p = json.loads(project.read_text())
        if (
            p["run"]["fluid_enabled"]
            or p["run"]["thermal_enabled"]
            or p["fluid"]["surface"]["enabled"]
        ):
            raise RuntimeError(
                "Prepared stationary input leaves a coupled subsystem enabled"
            )
        geometry = project.parent / p["geometry"]
        inputs = [project, geometry]
        inputs += [
            geometry.parent / b["uri"]
            for b in json.loads(geometry.read_text())["buffers"]
        ]
        hashes.update({str(path): sha(path) for path in inputs})
    if args.adapter == "amd":
        manifest = Path(os.environ["VK_DRIVER_FILES"])
        driver = Path(json.loads(manifest.read_text())["ICD"]["library_path"])
        hashes.update({str(manifest): sha(manifest), str(driver): sha(driver)})
    jobs = copy.deepcopy(plan["jobs"])
    for job in jobs:
        job["command"][0] = str(binary)
    common = {
        "controller_pid": os.getpid(),
        "input_and_binary_sha256": hashes,
        "predecessor_state": str(args.after_state),
        "predecessor_pid": args.after_pid,
        "plan": str(plan_path),
        "jobs": jobs,
        "adapter": args.adapter,
        "source_commit": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], text=True
        ).strip(),
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
    lease_path = root / (
        "native-amd-sequence.lock"
        if args.adapter == "amd"
        else "native-flow-sequence.lock"
    )
    with lease_path.open("a") as lease:
        fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
        for index, job in enumerate(jobs):
            processes = subprocess.run(
                ["ps", "-eo", "pid=,args="], capture_output=True, text=True, check=True
            ).stdout
            pattern = (
                r"^\s*\d+\s+(?:\S*/)?target/(?:debug|release)/examples/amd_hardware_probe\s"
                if args.adapter == "amd"
                else r"^\s*\d+\s+(?:\S*/)?target/(?:debug|release)/(?:examples/(?:conduction_flow|conduction_probe|applied_probe|hardware_probe)\s|mhd-workbench\s+--run-check\s)"
            )
            if any(
                re.search(
                    pattern,
                    line,
                )
                for line in processes.splitlines()
            ):
                raise RuntimeError("Another solver on this physical adapter is active")
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
                raise SystemExit(code)
            status = json.loads((Path(job["output"]) / "run-status.json").read_text())
            if not (
                status["status"] == "complete"
                and status["accepted_steps"] == job.get("requested_steps", 1200)
                and abs(status["time_s"] - job.get("end_time_s", 600e-6)) < 1e-10
            ):
                record(stage="invalid_completed_report", index=index, status=status)
                raise RuntimeError("Cold run did not complete requested interval")
            if args.adapter == "amd":
                audit = json.loads(
                    (Path(job["output"]) / "adapter-provenance.json").read_text()
                )
                if audit["vendor"] != 0x1002 or "AMD Radeon" not in audit["name"]:
                    raise RuntimeError(
                        "Native run did not use the isolated AMD adapter"
                    )
            record(stage="completed_job", index=index)
        record(stage="complete_needs_assessment", completed_jobs=len(jobs))


if __name__ == "__main__":
    main()
