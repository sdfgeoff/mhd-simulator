"""E3D-dimension stainless V6 with external DC contacts and transverse magnets.

This omits the conventional conductive heater block, which could shunt the
contacts. A heater arrangement that does not bridge the contacts is required.
"""

import argparse
import math

import manifold3d as cad
from pathlib import Path

from make_conduction_hardware import box, write_project
from make_hardware import lathe


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--name", default="v6-direct-dc-h010")
    p.add_argument("--domain-x-mm", type=float, default=5.5)
    p.add_argument("--domain-y-mm", type=float, default=4.5)
    p.add_argument("--domain-top-mm", type=float, default=14.0)
    p.add_argument("--flight-mm", type=float, default=5.0)
    p.add_argument("--cell", type=float, default=0.1)
    p.add_argument("--wall-sigma", type=float, default=1.3e6)
    p.add_argument("--remanence", type=float, default=0.95)
    p.add_argument("--magnet", choices=["cube10", "disc20-stack10"], default="cube10")
    p.add_argument("--orifice", type=float, choices=[0.4, 0.5, 0.6], default=0.4)
    p.add_argument(
        "--output", type=Path, default=Path("artifacts/hardware-research/cases")
    )
    args = p.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    rin, ro, length = 1.6, args.orifice / 2, 12.5
    land = {0.4: 0.6, 0.5: 0.9, 0.6: 1.2}[args.orifice]
    cone_top = land + (rin - ro) / math.tan(math.pi / 6)
    profile = [
        (3.0, length, False),
        (3.0, 6.5, False),
        (2.5, 6.5, False),
        (2.5, 5.0, False),
        (3.5, 5.0, True),
        (3.5, 2.0, True),
        (args.orifice * 1.25 + 2 * math.tan(math.radians(35)), 2.0, False),
        (args.orifice * 1.25, 0.0, False),
        (ro, 0.0, False),
        (ro, land, False),
        (rin, cone_top, False),
        (rin, length - 0.3, False),
        (rin + 0.3, length, False),
    ]
    liquid = [
        (0.0, 10.5, False),
        (rin, 10.5, False),
        (rin, cone_top, False),
        (ro, land, False),
        (ro, 0.0, False),
        (0.0, 0.0, False),
    ]
    # Match steel-bore triangle diagonals; orient() still sets liquid outward normals.
    # Reversed axial traversal otherwise introduces small raster disagreement at
    # the common conical boundary, despite nominally identical dimensions.
    parts = [
        ("Stainless V6 dimension approximation", lathe(profile), 14),
        ("Liquid solder", lathe(list(reversed(liquid))), 4),
        ("Positive copper jaw", box([1.5, 3.0, 2.6], [-4.25, 0.0, 3.5]), 16),
        ("Negative copper jaw", box([1.5, 3.0, 2.6], [4.25, 0.0, 3.5]), 17),
    ]
    if args.magnet == "cube10":
        magnets = [box([10.0, 10.0, 10.0], [0.0, sign * 9.75, 4.5]) for sign in [-1, 1]]
    else:
        # Two axially magnetized 20 x 5 mm discs abutting per pole; no inter-disc gap.
        # Homogeneous 10 mm stack is equivalent when magnetization is matched.
        magnets = [cad.Manifold.cylinder(10.0, 10.0, circular_segments=48, center=True)
                   .rotate([90, 0, 0]).translate([0.0, sign * 9.75, 4.5]) for sign in [-1, 1]]
        args.magnet_subdivisions = 2
    parts.extend((f"Bias magnet {i+1}", magnet, 18) for i, magnet in enumerate(magnets))
    write_project(parts, args)


if __name__ == "__main__":
    main()
