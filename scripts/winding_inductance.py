"""Independent thin round-wire Neumann energy estimate for generated CAD coils.

This is a lumped circuit parameter estimate, not a volumetric EM solver.
The softened filament uses a*exp(-1/4), matching low-frequency internal
inductance of a round, uniform-current wire. Skin/proximity corrections and
finite cross-section errors are not resolved. Compare discretizations and
the thin circular-loop asymptote before using circuit voltage estimates.
"""

import argparse
import json
import math
from pathlib import Path

import numpy as np


def discretize(points, maximum_segment_m):
    points = np.asarray(points, dtype=float)
    dl = np.diff(points, axis=0)
    count = np.maximum(
        1, np.ceil(np.linalg.norm(dl, axis=1) / maximum_segment_m).astype(int)
    )
    centres, segments = [], []
    for p, d, n in zip(points[:-1], dl, count):
        centres.extend(p + ((np.arange(n) + 0.5) / n)[:, None] * d)
        segments.extend(np.repeat((d / n)[None, :], n, axis=0))
    return np.array(centres), np.array(segments)


def inductance(points, radius_m, maximum_segment_m):
    centres, segments = discretize(points, maximum_segment_m)
    gmr = radius_m * math.exp(-0.25)
    result = 0.0
    for start in range(0, len(centres), 128):
        distance = centres[start : start + 128, None, :] - centres[None, :, :]
        distance = np.sqrt(np.einsum("ijk,ijk->ij", distance, distance) + gmr * gmr)
        dots = np.einsum("ik,jk->ij", segments[start : start + 128], segments)
        result += np.sum(dots / distance)
    return float(1e-7 * result), len(centres)


def mutual_inductance(first, second, maximum_segment_m):
    """Neumann mutual integral for separate closed filaments; no self softening."""
    a, da = discretize(first, maximum_segment_m)
    b, db = discretize(second, maximum_segment_m)
    result = 0.
    for start in range(0, len(a), 128):
        distance = a[start:start + 128, None, :] - b[None, :, :]
        distance = np.linalg.norm(distance, axis=2)
        if np.any(distance == 0):
            raise ValueError("Mutual winding filaments intersect at segment centres")
        result += np.sum((da[start:start + 128] @ db.T) / distance)
    return float(1e-7 * result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("project", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    p = json.loads(args.project.read_text())
    wires = p["electromagnetics"]["wires"]
    if len(wires) != 1:
        raise ValueError("This single-winding report requires exactly one source; use the matrix helper for multiple coils")
    wire = wires[0]
    points = np.array(p["cad_curves"][wire["cad_curve"]]["points_mm"]) * 0.001
    radius = wire["core_radius_mm"] * 0.001
    if np.linalg.norm(points[0] - points[-1]) > 1e-9:
        raise ValueError("Expected a closed winding including its return lead")
    results = []
    for spacing in [radius, radius / 2, radius / 4]:
        value, count = inductance(points, radius, spacing)
        results.append(
            {"maximum_segment_m": spacing, "segments": count, "inductance_h": value}
        )
    theta = np.linspace(0, 2 * np.pi, 2001)
    ring_r, wire_r = 0.01, 0.0001
    ring = np.column_stack((ring_r * np.cos(theta), ring_r * np.sin(theta), theta * 0))
    measured, _ = inductance(ring, wire_r, wire_r / 2)
    asymptote = 4e-7 * np.pi * ring_r * (math.log(8 * ring_r / wire_r) - 1.75)
    error = abs(measured / asymptote - 1)
    if error > 0.005:
        raise AssertionError(f"Circular loop asymptote discrepancy {error}")
    result = {
        "project": str(args.project),
        "method": __doc__,
        "winding": results,
        "circle_check": {
            "computed_h": measured,
            "thin_loop_asymptote_h": asymptote,
            "relative_error": error,
        },
        "warning": "Discretization agreement does not bound cross-section, skin, proximity, lead routing or conductor-reaction errors.",
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
