"""E3D-dimension stainless V6 with external DC contacts and transverse magnets.

This omits the conventional conductive heater block, which could shunt the
contacts. A heater arrangement that does not bridge the contacts is required.
"""

import argparse
import math
import json

import manifold3d as cad
import numpy as np
from pathlib import Path

from make_conduction_hardware import box, write_project
from make_hardware import heater, lathe


def as_solid(value):
    if hasattr(value, "to_mesh"):
        return value
    points = np.asarray(value, dtype=np.float32).reshape(-1, 3)
    vertices, indices = np.unique(points, axis=0, return_inverse=True)
    return cad.Manifold(cad.Mesh(vertices, indices.reshape(-1, 3).astype(np.uint32)))


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
    p.add_argument("--contact-band", choices=["hex", "upper", "tip", "immersed"], default="hex")
    p.add_argument("--magnet-z", type=float)
    p.add_argument("--pole-gap", type=float)
    p.add_argument("--pin-diameter-mm", type=float, default=0.5)
    p.add_argument("--pin-spacing-mm", type=float, default=1.0)
    p.add_argument("--pin-tip-mm", type=float, default=2.0)
    p.add_argument("--heater", action="store_true")
    p.add_argument("--heater-sigma", type=float, default=13e6)
    p.add_argument("--tip-contact-height-mm", type=float, default=.4)
    p.add_argument("--tip-contact-width-mm", type=float, default=.4)
    p.add_argument("--tip-contact-z-mm", type=float, default=.3)
    p.add_argument("--tip-flat-width-mm", type=float, help="Distance between opposed x contact planes on a custom V6-like tip")
    p.add_argument("--tip-flat-height-mm", type=float, default=.6, help="Machining reach above the exit for the optional tip flats")
    p.add_argument("--nozzle-flat-width-mm", type=float)
    p.add_argument("--magnet", choices=["cube10", "disc20-stack10", "disc", "block"], default="cube10")
    p.add_argument("--magnet-block-width-mm", type=float, default=10.)
    p.add_argument("--magnet-block-height-mm", type=float, default=5.)
    p.add_argument("--magnet-disc-diameter-mm", type=float, default=20.)
    p.add_argument("--magnet-stack-mm", type=float, default=10.)
    p.add_argument("--magnet-subdivisions", type=int)
    p.add_argument("--orifice", type=float, choices=[0.4, 0.5, 0.6], default=0.4)
    p.add_argument(
        "--output", type=Path, default=Path("artifacts/hardware-research/cases")
    )
    args = p.parse_args()
    if not all(math.isfinite(v) and v > 0 for v in [args.magnet_disc_diameter_mm, args.magnet_stack_mm]):
        raise ValueError("Magnet dimensions must be finite and positive")
    if not all(math.isfinite(v) and v > 0 for v in [args.magnet_block_width_mm, args.magnet_block_height_mm]):
        raise ValueError("Block magnet dimensions must be finite and positive")
    if args.magnet_subdivisions is None:
        args.magnet_subdivisions = 4 if args.magnet == "cube10" else 2
    if not 0 <= args.magnet_subdivisions <= 4:
        raise ValueError("Choose magnet subdivisions between zero and four")
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
    if args.nozzle_flat_width_mm is not None:
        width = args.nozzle_flat_width_mm
        if not math.isfinite(width) or not 2 * rin < width <= 7.:
            raise ValueError("Nozzle flats must clear the 3.2 mm bore and not exceed 7 mm")
        # Boolean machining keeps a valid transition where the round upper
        # neck can extend outside the narrowed collar. Vertex projection can
        # fold the horizontal transition ring when width < neck diameter.
        collar = as_solid(parts[0][1])
        for sign in [-1, 1]:
            collar -= box([20., 20., 3.01], [0., sign * (10. + width / 2), 3.495])
        parts[0] = ("V6 with two milled collar flats", collar, 14)
    tip_solid = None
    if args.tip_flat_width_mm is not None:
        width, reach = args.tip_flat_width_mm, args.tip_flat_height_mm
        bottom = args.tip_contact_z_mm - args.tip_contact_height_mm / 2
        top = args.tip_contact_z_mm + args.tip_contact_height_mm / 2
        bore_at_reach = ro + max(0., reach - land) * math.tan(math.pi / 6)
        outer_at_bottom = args.orifice * 1.25 + bottom * math.tan(math.radians(35))
        if (args.contact_band != "tip" or not all(math.isfinite(v) and v > 0 for v in [width, reach])
                or not top <= reach < 2. or bottom < 0 or width / 2 <= bore_at_reach
                or (width / 2)**2 + (args.tip_contact_width_mm / 2)**2 > outer_at_bottom**2):
            raise ValueError("Tip flats must clear the bore, contain the contact height, and provide the full contact width")
        # CAD boolean cuts only the external lands; the liquid/bore input stays
        # unchanged. Small overreach below z=0 avoids a coincident cutter bottom.
        tip_solid = as_solid(parts[0][1])
        for sign in [-1, 1]:
            tip_solid -= box([20., 20., reach + .02], [sign * (10. + width / 2), 0., reach / 2 - .01])
        if tip_solid.is_empty() or not tip_solid.volume() > 0:
            raise ValueError("Invalid custom tip geometry")
        parts[0] = ("Custom V6-like nozzle with opposed tip flats", tip_solid, 14)
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
        height, width, centre_z = args.tip_contact_height_mm, args.tip_contact_width_mm, args.tip_contact_z_mm
        if not all(math.isfinite(v) and v > 0 for v in [height, width, centre_z]) or centre_z - height / 2 < 0 or centre_z + height / 2 >= 2:
            raise ValueError("Tip contacts must be positive and stay below the shoulder and above the outlet")
        # Small external contacts localize current across the liquid orifice land.
        outer = cad.Manifold.cylinder(
            2.0,
            args.orifice * 1.25,
            args.orifice * 1.25 + 2 * math.tan(math.radians(35)),
            circular_segments=96,
        )
        for i, sign in enumerate([-1, 1]):
            if tip_solid is None:
                pad = box([4.5, width, height], [sign * 2.75, 0.0, centre_z]) - outer
            else:
                bore_at_contact_top = ro + max(0., centre_z + height / 2 - land) * math.tan(math.pi / 6)
                inner = (bore_at_contact_top + args.tip_flat_width_mm / 2) / 2
                pad = box([5. - inner, width, height], [sign * (5. + inner) / 2, 0., centre_z]) - tip_solid
            parts[i + 2] = (
                f"{'Positive' if sign < 0 else 'Negative'} tip copper contact",
                pad,
                16 + i,
            )
    if args.contact_band == "immersed":
        diameter, spacing, tip = args.pin_diameter_mm, args.pin_spacing_mm, args.pin_tip_mm
        if not all(math.isfinite(v) and v > 0 for v in [diameter, spacing, tip]):
            raise ValueError("Pin dimensions must be finite and positive")
        bore_radius = min(rin, ro + max(0., tip - land) * math.tan(math.pi / 6))
        if spacing <= diameter or (spacing + diameter) / 2 >= bore_radius or tip >= args.fill_height:
            raise ValueError("Pins must be disjoint, immersed and clear of the conical wall")
        pins = [cad.Manifold.cylinder(14. - tip, diameter / 2, circular_segments=48).translate([sign * spacing / 2, 0, tip]) for sign in [-1, 1]]
        raw = np.asarray(parts[1][1],dtype=np.float32).reshape(-1,3)
        vertices, indices = np.unique(raw,axis=0,return_inverse=True)
        pool = cad.Manifold(cad.Mesh(vertices,indices.reshape(-1,3).astype(np.uint32)))
        parts[1] = ("Liquid solder around immersed pins",pool-pins[0]-pins[1],4)
        for i,pin in enumerate(pins):
            parts[i+2] = (f"{'Positive' if i==0 else 'Negative'} immersed tungsten electrode",pin,16+i)
        args.terminal_material = "tungsten"
    disc_radius = args.magnet_disc_diameter_mm / 2
    half_height = args.magnet_block_height_mm / 2 if args.magnet == "block" else disc_radius
    tip_magnet_z = 1.8 - half_height if args.magnet != "cube10" else -8.2
    pin_magnet_z = 3.0 - half_height if args.magnet != "cube10" else -7.
    magnet_z = (
        args.magnet_z
        if args.magnet_z is not None
        else ({"upper": 8.5, "tip": tip_magnet_z, "immersed": pin_magnet_z}.get(args.contact_band, 4.5))
    )
    pole_gap = (
        args.pole_gap
        if args.pole_gap is not None
        else ({"tip":4.2,"immersed":8.5}.get(args.contact_band,9.5))
    )
    stack = 10.0 if args.magnet == "cube10" else args.magnet_stack_mm
    magnet_y = (pole_gap + stack) / 2
    if args.magnet == "cube10":
        magnets = [
            box([10.0, 10.0, 10.0], [0.0, sign * magnet_y, magnet_z])
            for sign in [-1, 1]
        ]
    elif args.magnet == "block":
        magnets = [box([args.magnet_block_width_mm, stack, args.magnet_block_height_mm],
                       [0., sign * magnet_y, magnet_z]) for sign in [-1, 1]]
    else:
        # A homogeneous stack represents abutting discs with matched axial
        # magnetization. Stock-disc thickness must divide the requested stack.
        magnets = [
            cad.Manifold.cylinder(stack, disc_radius, circular_segments=48, center=True)
            .rotate([90, 0, 0])
            .translate([0.0, sign * magnet_y, magnet_z])
            for sign in [-1, 1]
        ]
    if args.heater:
        if not math.isfinite(args.heater_sigma) or args.heater_sigma <= 0:
            raise ValueError("Heater conductivity must be finite and positive")
        parts.append(("Simplified aluminium heater block", heater(6.5, 18.0), 8))
    parts.extend((f"Bias magnet {i+1}", magnet, 18) for i, magnet in enumerate(magnets))
    project = write_project(parts, args)
    if args.air_return_periodic_z or args.heater:
        data = json.loads(project.read_text())
        if args.air_return_periodic_z:
            data["fluid"]["periodic"][2] = True
        if args.heater:
            next(m for m in data["materials"] if m["id"] == 8)["conductivity_s_m"] = args.heater_sigma
        project.write_text(json.dumps(data, indent=2) + "\n")
    project.with_name(project.stem + "-assumptions.json").write_text(
        json.dumps(
            {
                "configuration": {
                    k: str(v) if isinstance(v, Path) else v
                    for k, v in vars(args).items()
                },
                "limits": [
                    "Screening CAD; threads, fillets, heater bores and mounts omitted. A bulk heater block is included only with --heater; its conductivity is an explicit hot screening assumption, not a sourced temperature fit.",
                    "Cropping conductors requires a cold-domain comparison; periodic z is an artificial air return when the upper nozzle is cropped. It must be checked against a full-height fluid domain and stopped before liquid reaches a periodic boundary.",
                ],
            },
            indent=2,
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
