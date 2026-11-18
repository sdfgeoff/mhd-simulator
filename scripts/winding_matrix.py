"""Lumped air-core winding matrix from actual CAD centreline curves.

Thin-wire Neumann integrals, not a volumetric EM solver or loaded driver model.
Self terms retain the round-wire low-frequency internal-inductance convention.
Mutual terms are unsoftened and require physically separate windings.
"""

import argparse
import json
from pathlib import Path

import numpy as np

from winding_inductance import inductance, mutual_inductance


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("project", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    project = json.loads(args.project.read_text())
    wires = project["electromagnetics"]["wires"]
    if not wires:
        raise ValueError("No winding sources")
    curves, radii = [], []
    for wire in wires:
        points = np.array(project["cad_curves"][wire["cad_curve"]]["points_mm"]) * .001
        if wire["reverse"]:
            points = points[::-1]
        if np.linalg.norm(points[0] - points[-1]) > 1e-9:
            raise ValueError("Expected closed winding and return lead")
        curves.append(points)
        radii.append(wire["core_radius_mm"] * .001)
    results = []
    for spacing in [min(radii) / 2, min(radii) / 4]:
        matrix = np.zeros((len(wires), len(wires)))
        reciprocity_error = 0.
        for i, first in enumerate(curves):
            matrix[i, i] = inductance(first, radii[i], spacing)[0]
            for j in range(i):
                forward = mutual_inductance(first, curves[j], spacing)
                backward = mutual_inductance(curves[j], first, spacing)
                reciprocity_error = max(reciprocity_error, abs(forward - backward))
                matrix[i, j] = matrix[j, i] = (forward + backward) / 2
        if np.linalg.eigvalsh(matrix).min() <= 0:
            raise ValueError("Nonpositive winding energy matrix")
        results.append({"maximum_segment_m": spacing, "matrix_h": matrix.tolist(),
                        "coupling_coefficients": (matrix / np.sqrt(np.outer(matrix.diagonal(), matrix.diagonal()))).tolist(),
                        "reciprocity_absolute_error_h": reciprocity_error})
    # Independent far-separated circular-loop dipole limit (error O(radius²/z²)).
    theta = np.linspace(0, 2 * np.pi, 1001)
    radius, separation = .01, .3
    ring = np.column_stack((radius * np.cos(theta), radius * np.sin(theta), theta * 0))
    other = ring + [0, 0, separation]
    measured = mutual_inductance(ring, other, .00025)
    dipole = 4e-7 * np.pi**2 * radius**4 / (2 * separation**3)
    error = abs(measured / dipole - 1)
    if error > .005:
        raise AssertionError(f"Far-loop dipole discrepancy {error}")
    result = {"project": str(args.project), "method": __doc__,
              "wire_names": [w["name"] for w in wires], "refinement": results,
              "far_loop_check": {"measured_h": measured, "dipole_limit_h": dipole, "relative_error": error},
              "limits": ["No induced-metal reaction, magnetic cores, skin/proximity effect or physical lead uncertainty.",
                         "Discretization agreement does not qualify insulation clearances or winding manufacture.",
                         "Air matrix alone cannot establish loaded resonance, bus voltage, cooling or controllability."]}
    with args.output.open("x") as f:
        json.dump(result, f, indent=2)
        f.write("\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
