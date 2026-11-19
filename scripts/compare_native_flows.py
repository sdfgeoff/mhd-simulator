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
from native_field_io import read_native_fields


def frames(directory, available=False):
    status_path = directory / "run-status.json"
    status = (json.loads(status_path.read_text()) if status_path.exists()
              else {"status": "unfinished", "scope": "No terminal run status exists"})
    if not available and status["status"] != "complete":
        raise ValueError(f"Comparison requires a completed run: {directory}")
    result = {}
    for path in directory.glob("frame-*/metadata.json"):
        data = json.loads(path.read_text())
        if data["format_version"] != 1 or not data["moving_interface"]:
            raise ValueError("Expected version-1 moving-interface exports")
        result[data["step"]] = (path, data)
    return status, result


def comparison_grid(a, b, overlap):
    if a["cell_m"] != b["cell_m"]:
        raise ValueError(
            "Different grid spacings require a separate refinement diagnostic"
        )
    if not overlap and any(a[k] != b[k] for k in ["dims", "origin_m"]):
        raise ValueError("Different domains require explicit --overlap")
    h = a["cell_m"]
    offset = (np.array(a["origin_m"]) - b["origin_m"]) / h
    integer_offset = np.rint(offset).astype(int)
    if np.max(abs(offset - integer_offset)) > 1e-4:
        raise ValueError(
            "Native cell centres do not align; interpolation is not performed"
        )
    start_a = np.maximum(0, -integer_offset)
    start_b = np.maximum(0, integer_offset)
    dims = np.minimum(np.array(a["dims"]) - start_a, np.array(b["dims"]) - start_b)
    if np.any(dims <= 0):
        raise ValueError("No common native cells")
    slices = [
        tuple(slice(int(s), int(s + n)) for s, n in zip(start[::-1], dims[::-1]))
        for start in [start_a, start_b]
    ]
    grid = {
        "dims": dims.tolist(),
        "origin_m": (np.array(a["origin_m"]) + start_a * h).tolist(),
        "cell_m": h,
        "scope": "common aligned native cells; no interpolation",
    }
    return slices, grid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("driven", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--overlap",
        action="store_true",
        help="Compare only common aligned cells of different-size domains",
    )
    parser.add_argument("--match-time", action="store_true",
                        help="Match exports by physical time when timestep counts differ")
    parser.add_argument("--available", action="store_true",
                        help="Explicit partial comparison of shared accepted exports; permits unfinished/failed runs")
    args = parser.parse_args()
    reference_status, reference = frames(args.reference, args.available)
    driven_status, driven = frames(args.driven, args.available)
    if args.match_time:
        def by_time(items):
            result = {round(meta["time_s"] * 1e12): (path, meta)
                      for path, meta in items.values()}
            if len(result) != len(items):
                raise ValueError("Multiple exports map to one physical time")
            return result
        reference, driven = by_time(reference), by_time(driven)
    if args.available:
        common = reference.keys() & driven.keys()
        reference = {k: reference[k] for k in common}
        driven = {k: driven[k] for k in common}
    if not reference or reference.keys() != driven.keys():
        raise ValueError("Require the same set of accepted exported steps")
    rows = []
    for step in sorted(reference):
        (pa, a), (pb, b) = reference[step], driven[step]
        slices, grid = comparison_grid(a, b, args.overlap)
        if abs(a["time_s"] - b["time_s"]) > 1e-12:
            raise ValueError(f"Clock mismatch at step {step}")
        fields = [
            read_native_fields(path.parent / "fluid.bin", (*meta["dims"][::-1], 16))
            for path, meta in [(pa, a), (pb, b)]
        ]
        whole_volumes = [
            float(f[..., 14].sum(dtype=float) * grid["cell_m"] ** 3 * 1e12)
            for f in fields
        ]
        first, second = [f[s] for f, s in zip(fields, slices)]
        nx, ny, nz = grid["dims"]
        alpha_a, alpha_b = first[..., 14].astype(float), second[..., 14].astype(float)
        if not all(
            np.isfinite(f[..., :3]).all() and np.isfinite(f[..., 14]).all()
            for f in [first, second]
        ):
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
            a[k] == b[k]
            for k in ["bodies", "materials", "cad_curves", "fluid", "run", "geometry"]
        )
        row = {
            "step": a["step"],
            "candidate_step": b["step"],
            "time_us": a["time_s"] * 1e6,
            "comparison_grid": grid,
            "exported_configuration_matches_except_wire_currents": current_only,
            "alpha_L1_volume_difference_nl": float(
                np.abs(alpha_a - alpha_b).sum() * cell * 1e12
            ),
            "alpha_RMS_difference": float(np.sqrt(np.mean((alpha_a - alpha_b) ** 2))),
            "common_liquid_weighted_velocity_RMS_difference_m_s": float(
                np.sqrt(np.sum(weight * np.sum(du * du, axis=-1)) / weight.sum())
            ),
            "static_open_mask_identical": bool(
                np.array_equal(first[..., 12], second[..., 12])
            ),
        }
        z = grid["origin_m"][2] + (np.arange(nz) + 0.5) * a["cell_m"]
        below = np.clip(-(z - a["cell_m"] / 2) / a["cell_m"], 0, 1)
        for label, alpha, field in [
            ("reference", alpha_a, first),
            ("driven", alpha_b, second),
        ]:
            w = np.clip(alpha, 0, 1)
            row[label] = {
                "liquid_volume_nl": float(alpha.sum() * cell * 1e12),
                "whole_domain_liquid_volume_nl": whole_volumes[
                    0 if label == "reference" else 1
                ],
                "below_tip_nl": float(
                    np.sum(alpha * below[:, None, None]) * cell * 1e12
                ),
                "liquid_weighted_RMS_speed_m_s": float(
                    np.sqrt(
                        np.sum(w * np.sum(field[..., :3].astype(float) ** 2, axis=-1))
                        / w.sum()
                    )
                ),
                "liquid_centre_z_mm": float(
                    np.sum(w * z[:, None, None]) / w.sum() * 1000
                ),
            }
        rows.append(row)
    result = {
        "method": __doc__,
        "comparison_scope": "shared accepted exports only; partial run comparison" if args.available else "completed matched runs",
        "partial_comparison_requested": args.available,
        "configuration_comparison_scope": "Exported metadata only. External research electrode waveforms require separate native run-status checks.",
        "export_matching": "physical time within1ps" if args.match_time else "same accepted step",
        "reference": str(args.reference),
        "driven": str(args.driven),
        "reference_status": reference_status,
        "driven_status": driven_status,
        "frames": rows,
        "final_field_sha256": {
            str(p.parent / "fluid.bin"): hashlib.sha256(f).hexdigest()
            for (p, _), f in zip(
                [reference[max(reference)], driven[max(driven)]], fields
            )
        },
        "limits": [
            "Cell-centred diagnostics are not the solver's discrete face kinetic energy.",
            "Comparisons do not establish grid convergence, driver feasibility or repetitive jetting.",
        ],
    }
    with args.output.open("x") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
    print(json.dumps(rows[-1], indent=2))


if __name__ == "__main__":
    main()
