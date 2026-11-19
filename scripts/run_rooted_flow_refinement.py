"""Run coarse exterior-gas comparison, then a gated coupled space/time refinement."""

import json, os, re, shutil, subprocess, sys
from pathlib import Path
from datetime import datetime

root = Path("artifacts/hardware-research")
state = root / "rooted-flow-refinement-state.json"
env = {**os.environ, "TMPDIR": str(Path.cwd() / "artifacts/build-temp")}


def record(**values):
    temporary = state.with_suffix(".new")
    temporary.write_text(
        json.dumps(
            {"updated_at": datetime.now().astimezone().isoformat(), **values}, indent=2
        )
        + "\n"
    )
    temporary.replace(state)


def quota(stage):
    result = subprocess.run(
        [sys.executable, "/home/geoffrey/bin/weekly_remaining.py"],
        capture_output=True,
        text=True,
    )
    (root / ("weekly-before-" + stage + ".txt")).write_text(
        result.stdout + result.stderr
    )
    match = re.search(r"Weekly remaining:\s*(\d+)%", result.stdout)
    if result.returncode or not match:
        raise RuntimeError("Live weekly budget check failed")
    if int(match.group(1)) <= 40:
        record(stage="weekly_stop", weekly_remaining=int(match.group(1)))
        raise SystemExit(0)


def run(stage, command, log_name):
    print("Starting", stage, flush=True)
    with (root / log_name).open("w") as log:
        process = subprocess.Popen(
            command, stdout=log, stderr=subprocess.STDOUT, env=env
        )
        record(stage=stage, pid=process.pid, command=command)
        code = process.wait()
    record(stage=stage + "_terminal", returncode=code)
    if code:
        raise RuntimeError(stage + " failed; inspect " + log_name)


def report(directory):
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
            str(directory),
            "--plot",
            "--pool-view",
        ],
        stdout=(root / (directory.name + "-automatic-report.log")).open("w"),
        stderr=subprocess.STDOUT,
        check=True,
        env=env,
    )


zero = root / "m6-rooted08-flow0-vented-tol1e4-h010"
zstatus = json.loads((zero / "run-status.json").read_text())
zdata = json.loads((zero / "flow-analysis.json").read_text())
if (
    zstatus["status"] != "complete"
    or max(r["below_tip_nl"] for r in zdata["frames"]) > 1
    or any(r["detached_below_tip"] for r in zdata["frames"])
):
    raise RuntimeError("Zero-drive comparison not accepted")
coarse = root / "m6-rooted08-flow110-r25-compactgas-h010"
fine = root / "m6-rooted08-flow110-r25-compactgas-h0050"
if coarse.exists() or fine.exists():
    raise RuntimeError("Refinement output exists; inspect before repeating")
quota("compactgas")
run(
    "running_compactgas",
    [
        "target/debug/examples/conduction_flow",
        str(root / "cases/m6-rooted08-flow-compactgas-h010.mhd.json"),
        "110",
        "250",
        "1200",
        str(coarse),
        "0",
        ".25",
        "3",
        "0",
        "300",
    ],
    coarse.name + ".log",
)
report(coarse)
gate = root / "rooted-compactgas-gate.json"
subprocess.run(
    [
        sys.executable,
        "scripts/compare_rooted_gas_domain.py",
        str(coarse),
        "--output",
        str(gate),
    ],
    check=True,
    env=env,
)
# Completed controls can be archived only after their reports exist. The helper
# verifies decoded SHA before removing raw fields, and all report readers support it.
raw = list(zero.glob("frame-*/fluid.bin"))
if raw:
    subprocess.run(
        [sys.executable, "scripts/archive_native_field.py", *map(str, raw)],
        check=True,
        env=env,
    )
quota("finegrid")
if shutil.disk_usage(root).free < 4_000_000_000:
    raise RuntimeError(
        "Insufficient free disk for the finer trial; archive completed owned artifacts before proceeding"
    )
run(
    "running_finegrid",
    [
        "target/debug/examples/conduction_flow",
        str(root / "cases/m6-rooted08-flow-compactgas-h0050.mhd.json"),
        "110",
        "250",
        "1320",
        str(fine),
        "0",
        ".25",
        "1.5",
        "0",
        "300",
        "3",
    ],
    fine.name + ".log",
)
report(fine)
record(
    stage="complete_needs_assessment",
    coarse=str(coarse),
    fine=str(fine),
    gate=str(gate),
    scope="Coarse5%primary-metric gas-domain gate; fine run changes50umspace/1.5usforcingstep and3ustail. Actual finaltime3.6015ms. No automatic physical qualification.",
)
print(
    "Refinement complete; numerical and hardware assessment still required.", flush=True
)
