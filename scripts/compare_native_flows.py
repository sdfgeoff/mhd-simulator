"""Compare completed, matched-grid native fluid exports without interpolation.

Cell-centred alpha/velocity diagnostics only; not an additional fluid solver.
Alpha L1 is an absolute phase-volume difference, not net mass loss.
"""

import argparse
import copy
import hashlib
import json
from pathlib import Path

import numpy as np


def frames(directory):
    status = json.loads((directory / "run-status.json").read_text())
    if status["status"] != "complete":
        raise ValueError(f"Comparison requires a completed run: {directory}")
    result = {}
    for path in directory.glob("frame-*/metadata.json"):
        data = json.loads(path.read_text())
        if data["format_version"] != 1 or not data["moving_interface"]:
            raise ValueError("Expected version-1 moving-interface exports")
        result[data["step"]] = (path, data)
    return status, result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("driven", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    reference_status, reference = frames(args.reference)
    driven_status, driven = frames(args.driven)
    if not reference or reference.keys() != driven.keys():
        raise ValueError("Require the same set of accepted exported steps")
    rows = []
    for step in sorted(reference):
        (pa, a), (pb, b) = reference[step], driven[step]
        if any(a[k] != b[k] for k in ["dims", "origin_m", "cell_m"]):
            raise ValueError(f"Grid mismatch at step {step}")
        if abs(a["time_s"] - b["time_s"]) > 1e-12:
            raise ValueError(f"Clock mismatch at step {step}")
        nx, ny, nz = a["dims"]
        first = np.memmap(pa.parent / "fluid.bin", dtype="<f4", mode="r").reshape(nz, ny, nx, 16)
        second = np.memmap(pb.parent / "fluid.bin", dtype="<f4", mode="r").reshape(nz, ny, nx, 16)
        alpha_a, alpha_b = first[..., 14].astype(float), second[..., 14].astype(float)
        if not all(np.isfinite(f[..., :3]).all() and np.isfinite(f[..., 14]).all()
                   for f in [first, second]):
            raise ValueError(f"Nonfinite accepted fluid state at step {step}")
        weight = np.minimum(np.clip(alpha_a, 0, 1), np.clip(alpha_b, 0, 1))
        if weight.sum() <= 0:
            raise ValueError("No common liquid volume for the velocity diagnostic")
        du = second[..., :3].astype(float) - first[..., :3]
        cell = a["cell_m"] ** 3
        source_configs = [copy.deepcopy(m["electromagnetics"]) for m in [a, b]]
        for config in source_configs:
            for wire in config["wires"]:
                wire.pop("current")
        current_only = source_configs[0] == source_configs[1] and all(
            a[k] == b[k] for k in ["bodies", "materials", "cad_curves", "fluid", "run", "geometry"])
        row = {"step": step, "time_us": a["time_s"] * 1e6,
               "exported_configuration_matches_except_wire_currents": current_only,
               "alpha_L1_volume_difference_nl": float(np.abs(alpha_a - alpha_b).sum() * cell * 1e12),
               "alpha_RMS_difference": float(np.sqrt(np.mean((alpha_a - alpha_b) ** 2))),
               "common_liquid_weighted_velocity_RMS_difference_m_s": float(np.sqrt(np.sum(weight * np.sum(du * du, axis=-1)) / weight.sum())),
               "static_open_mask_identical": bool(np.array_equal(first[..., 12], second[..., 12]))}
        z = a["origin_m"][2] + (np.arange(nz) + .5) * a["cell_m"]
        below = np.clip(-(z - a["cell_m"] / 2) / a["cell_m"], 0, 1)
        for label, alpha, field in [("reference", alpha_a, first), ("driven", alpha_b, second)]:
            w = np.clip(alpha, 0, 1)
            row[label] = {"liquid_volume_nl": float(alpha.sum() * cell * 1e12),
                          "below_tip_nl": float(np.sum(alpha * below[:, None, None]) * cell * 1e12),
                          "liquid_weighted_RMS_speed_m_s": float(np.sqrt(np.sum(w * np.sum(field[..., :3].astype(float) ** 2, axis=-1)) / w.sum())),
                          "liquid_centre_z_mm": float(np.sum(w * z[:, None, None]) / w.sum() * 1000)}
        rows.append(row)
    result = {"method": __doc__, "reference": str(args.reference), "driven": str(args.driven),
              "reference_status": reference_status, "driven_status": driven_status,
              "frames": rows, "final_field_sha256": {str(p.parent / "fluid.bin"): hashlib.sha256((p.parent / "fluid.bin").read_bytes()).hexdigest()
                                                       for p, _ in [reference[max(reference)], driven[max(driven)]]},
              "limits": ["Cell-centred diagnostics are not the solver's discrete face kinetic energy.",
                         "Comparisons do not establish grid convergence, driver feasibility or repetitive jetting."]}
    with args.output.open("x") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
    print(json.dumps(rows[-1], indent=2))


if __name__ == "__main__":
    main()
