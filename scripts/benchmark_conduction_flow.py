"""Replay identical full-assembly steps with two solver builds on one GPU.

Pass binaries built with the same Cargo profile. Set VK_DRIVER_FILES to select
the adapter. Each replay exports its accepted state and retains its diagnostics.
"""
import argparse
import csv
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "hardware/concept1-sturdy-tip-review/cad-drill118-060-v1/flow-plan-forward120.json"


def replay(binary, destination, steps, command, sample_memory):
    if destination.exists():
        raise FileExistsError(destination)
    args = list(command)
    args[0], args[4], args[5] = str(binary.resolve()), str(steps), str(destination)
    started = time.monotonic()
    peak_mib = None
    with destination.with_suffix(".log").open("x") as log:
        with subprocess.Popen(args, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT) as process:
            if sample_memory:
                while process.poll() is None:
                    usage = subprocess.check_output(["nvidia-smi",
                        "--query-compute-apps=pid,used_gpu_memory",
                        "--format=csv,noheader,nounits"], text=True)
                    for line in usage.splitlines():
                        pid, mib = (part.strip() for part in line.split(",", 1))
                        if pid == str(process.pid) and mib.isdecimal():
                            peak_mib = max(peak_mib or 0, int(mib))
                    try:
                        process.wait(timeout=0.5)
                    except subprocess.TimeoutExpired:
                        pass
            if process.wait() != 0:
                raise RuntimeError(f"Replay failed; see {log.name}")

    elapsed = time.monotonic() - started
    status = json.loads((destination / "run-status.json").read_text())
    if status["status"] != "complete" or status["accepted_steps"] != steps:
        raise RuntimeError(f"Incomplete replay in {destination}")
    with (destination / "iteration-costs.csv").open() as file:
        rows = list(csv.DictReader(file))
    if len(rows) != steps:
        raise ValueError("Incomplete timestep timings")
    return {"process_wall_s": elapsed, "sampled_process_peak_mib": peak_mib, **{
        key.removesuffix("_ms") + "_s": sum(float(row[key]) for row in rows) / 1000
        for key in rows[0] if key.endswith("_ms")
    }}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--steps", type=int, default=3)
    parser.add_argument("--pairs", type=int, default=2)
    parser.add_argument("--sample-nvidia", action="store_true",
                        help="Sample per-process GPU memory using nvidia-smi")
    args = parser.parse_args()
    if min(args.steps, args.pairs) < 1:
        parser.error("Positive steps and pair count required")
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    command = json.loads(PLAN.read_text())["jobs"][0]["command"]
    results = {}
    for pair in range(args.pairs):
        builds = [("baseline", args.baseline), ("candidate", args.candidate)]
        for label, binary in (reversed(builds) if pair % 2 else builds):
            name = f"{label}-{pair + 1}"
            results[name] = replay(binary, root / name, args.steps, command, args.sample_nvidia)
            print(name, results[name], flush=True)
            (root / "benchmark-results.json").write_text(json.dumps(results, indent=2) + "\n")


if __name__ == "__main__":
    main()
