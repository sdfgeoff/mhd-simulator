"""E3D-dimension stainless V6 with external DC contacts and transverse magnets.

This omits the conventional conductive heater block, which could shunt the
contacts. A heater arrangement that does not bridge the contacts is required.
"""

import argparse
import math
import json

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
    p.add_argument("--fill-height", type=float, default=10.5)
    p.add_argument("--air-return-periodic-z", action="store_true")
    p.add_argument("--wall-sigma", type=float, default=1.3e6)
    p.add_argument("--remanence", type=float, default=0.95)
    p.add_argument("--contact-band", choices=["hex", "upper", "tip"], default="hex")
    p.add_argument("--magnet-z", type=float)
    p.add_argument("--pole-gap", type=float)
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
    if not cone_top <= args.fill_height <= length - 0.3:
        raise ValueError(
            "Fill height must lie in the cylindrical bore, above the internal cone"
        )
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
        (0.0, args.fill_height, False),
        (rin, args.fill_height, False),
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
    if args.contact_band == "upper":
        # Concave patches contact +/-x faces of the M6 barrel from z=6.5 to 10.25.
        # They stop below the initial free surface rather than energizing air-filled bore.
        barrel = cad.Manifold.cylinder(13, 3.0, circular_segments=96).translate(
            [0, 0, -0.25]
        )
        for i, sign in enumerate([-1, 1]):
            pad = box([2.4, 3.0, 3.75], [sign * 3.8, 0.0, 8.375]) - barrel
            parts[i + 2] = (
                f"{'Positive' if sign < 0 else 'Negative'} upper copper contact",
                pad,
                16 + i,
            )
    if args.contact_band == "tip":
        # Small external contacts localize current across the liquid orifice land.
        outer = cad.Manifold.cylinder(
            2.0,
            args.orifice * 1.25,
            args.orifice * 1.25 + 2 * math.tan(math.radians(35)),
            circular_segments=96,
        )
        for i, sign in enumerate([-1, 1]):
            pad = box([4.5, 0.4, 0.4], [sign * 2.75, 0.0, 0.3]) - outer
            parts[i + 2] = (
                f"{'Positive' if sign < 0 else 'Negative'} tip copper contact",
                pad,
                16 + i,
            )
    magnet_z = (
        args.magnet_z
        if args.magnet_z is not None
        else ({"upper": 8.5, "tip": -8.2}.get(args.contact_band, 4.5))
    )
    pole_gap = (
        args.pole_gap
        if args.pole_gap is not None
        else (4.2 if args.contact_band == "tip" else 9.5)
    )
    magnet_y = pole_gap / 2 + 5.0
    if args.magnet == "cube10":
        magnets = [
            box([10.0, 10.0, 10.0], [0.0, sign * magnet_y, magnet_z])
            for sign in [-1, 1]
        ]
    else:
        # Two axially magnetized 20 x 5 mm discs abutting per pole; no inter-disc gap.
        # Homogeneous 10 mm stack is equivalent when magnetization is matched.
        magnets = [
            cad.Manifold.cylinder(10.0, 10.0, circular_segments=48, center=True)
            .rotate([90, 0, 0])
            .translate([0.0, sign * magnet_y, magnet_z])
            for sign in [-1, 1]
        ]
        args.magnet_subdivisions = 2
    parts.extend((f"Bias magnet {i+1}", magnet, 18) for i, magnet in enumerate(magnets))
    project = write_project(parts, args)
    if args.air_return_periodic_z:
        data = json.loads(project.read_text())
        data["fluid"]["periodic"][2] = True
        project.write_text(json.dumps(data, indent=2) + "\n")
    project.with_name(project.stem + "-assumptions.json").write_text(
        json.dumps(
            {
                "configuration": {
                    k: str(v) if isinstance(v, Path) else v
                    for k, v in vars(args).items()
                },
                "limits": [
                    "Screening CAD; threads, fillets, heater and mounts omitted.",
                    "Cropping conductors requires a cold-domain comparison; periodic z is an artificial air return when the upper nozzle is cropped. It must be checked against a full-height fluid domain and stopped before liquid reaches a periodic boundary.",
                ],
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
