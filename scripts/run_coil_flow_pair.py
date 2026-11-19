"""Guarded AMD queue for the normal unified RunState coil-flow controls."""
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
    parser.add_argument("--after-pid", type=int, required=True)
    args = parser.parse_args()
    root = Path("artifacts/hardware-research")
    plan_path = root / "coil-low-fill-flow-v1-plan.json"
    predecessor = root / "unipolar-coil-fast10-v2-amd-state.json"
    cold_plan = root / "unipolar-coil-fast10-v2-plan.json"
    state = root / "coil-low-fill-flow-v1-amd-state.json"
    plan = json.loads(plan_path.read_text())
    if state.exists() or any(Path(j["output"]).exists() or Path(j["output"] + ".log").exists() for j in plan["jobs"]):
        raise RuntimeError("Existing state/output; inspect before repeating")
    manifest = Path(os.environ["VK_DRIVER_FILES"])
    inputs = [plan_path, cold_plan, Path("target/debug/mhd-workbench"),
              Path("target/debug/examples/adapter_inventory"), manifest,
              Path(json.loads(manifest.read_text())["ICD"]["library_path"])]
    for job in plan["jobs"]:
        assert sha(job["project"]) == job["project_sha256"]
        path = Path(job["project"])
        p = json.loads(path.read_text())
        assert p["run"]["fluid_enabled"] and p["fluid"]["surface"]["enabled"]
        assert not p["run"]["thermal_enabled"] and p["run"]["prescribed_jet"] is None
        geometry = path.parent / p["geometry"]
        inputs += [path, geometry] + [geometry.parent / b["uri"] for b in json.loads(geometry.read_text())["buffers"]]
    common = {"controller_pid": os.getpid(), "predecessor": str(predecessor),
              "predecessor_pid": args.after_pid, "plan": str(plan_path),
              "input_and_binary_sha256": {str(p): sha(p) for p in inputs},
              "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()}

    def record(**values):
        temp = state.with_suffix(".new")
        temp.write_text(json.dumps({**common, "updated_at": datetime.now().astimezone().isoformat(), **values}, indent=2) + "\n")
        temp.replace(state)

    record(stage="waiting_for_predecessor")
    try:
        while True:
            previous = json.loads(predecessor.read_text())
            try:
                os.kill(args.after_pid, 0)
                alive = True
            except ProcessLookupError:
                alive = False
            if previous["stage"] == "complete_needs_assessment" and not alive:
                break
            if not alive or previous["stage"] in ["failed", "weekly_stop", "predecessor_not_complete", "invalid_completed_report"]:
                record(stage="predecessor_not_complete", observed_state=previous)
                return
            time.sleep(30)
        # CPU-only analysis verifies completed native EM results and decay/work gates.
        subprocess.run([sys.executable, "scripts/analyze_unipolar_coil.py", "--plan", str(cold_plan)], check=True)
        result = json.loads((root / "unipolar-coil-fast10-v2-comparison.json").read_text())
        custom = next(c for c in result["cases"] if c["geometry"] == "custom")
        if max(abs(v) for v in custom["loaded_terminal_interval_mean_voltage_range_V"]) > 48 or custom["rms_A_at_assumed200Hz"] > 30:
            record(stage="cold_result_needs_review", cold_result=custom)
            return
        with (root / "native-amd-sequence.lock").open("a") as lease:
            fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
            for index, job in enumerate(plan["jobs"]):
                if any(sha(path) != value for path, value in common["input_and_binary_sha256"].items()):
                    raise RuntimeError("Frozen flow inputs/binaries changed")
                # Check explicit per-process adapter environment as well as the lease.
                for proc in Path("/proc").iterdir():
                    if not proc.name.isdigit() or int(proc.name) == os.getpid():
                        continue
                    try:
                        command = (proc / "cmdline").read_bytes().split(b"\0")
                        env = (proc / "environ").read_bytes().split(b"\0")
                    except (FileNotFoundError, PermissionError, ProcessLookupError):
                        continue
                    if command and Path(os.fsdecode(command[0])).name in ["amd_hardware_probe", "hardware_probe", "conduction_flow", "mhd-workbench"] and os.fsencode("VK_DRIVER_FILES=" + str(manifest)) in env:
                        raise RuntimeError("Another solver on AMD is active")
                quota = subprocess.check_output([sys.executable, "/home/geoffrey/bin/weekly_remaining.py"], text=True)
                match = re.search(r"Weekly remaining:\s*(\d+)%", quota)
                if not match:
                    raise RuntimeError("Live weekly reading unavailable")
                if int(match[1]) <= 40:
                    record(stage="weekly_stop", weekly_remaining=int(match[1]))
                    return
                audit = json.loads(subprocess.check_output(["target/debug/examples/adapter_inventory", "--expect-amd"], text=True))
                if len(audit["vulkan_adapters"]) != 1 or audit["vulkan_adapters"][0]["vendor"] != 0x1002:
                    raise RuntimeError("Expected isolated AMD adapter")
                log_path = Path(job["output"] + ".log")
                with log_path.open("x") as log:
                    process = subprocess.Popen(job["command"], stdout=log, stderr=subprocess.STDOUT)
                    record(stage="running", index=index, pid=process.pid, weekly_remaining=int(match[1]))
                    code = process.wait()
                output = Path(job["output"])
                if code:
                    raise RuntimeError(f"Native unified flow failed with exit{code}")
                if "GPU: " + audit["default_headless_selected_name"] not in log_path.read_text():
                    raise RuntimeError("Native flow adapter differs from preflight")
                report = json.loads((output / "run-status.json").read_text())
                if report["status"] != "complete" or report["accepted_steps"] != job["requested_steps"] or abs(report["time_s"] - job["end_time_s"]) > 1e-9:
                    raise RuntimeError("Native flow interval incomplete")
                if not report["electromagnetics"] or not report["fluids"] or report["thermal"]:
                    raise RuntimeError("Unexpected native coupled subsystem configuration")
                (output / "adapter-provenance.json").write_text(json.dumps(audit, indent=2) + "\n")
                subprocess.run([sys.executable, "scripts/hardware_flow_report.py", str(output), "--plot", "--pool-view"], check=True)
                record(stage="completed_job", index=index)
        record(stage="complete_needs_assessment", completed_jobs=len(plan["jobs"]))
    except Exception as error:
        record(stage="failed", error=str(error))
        raise


if __name__ == "__main__":
    main()
