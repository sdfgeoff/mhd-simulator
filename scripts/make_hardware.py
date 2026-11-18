"""Generate loadable design-study CAD and projects; physics stays in Rust/WGSL.

V6/Volcano dimensions: E3D released drawings V6 rev8.1 and Volcano rev5.2.
Threads/fillets/dot marks are simplified and must be bracketed/refined.
"""

import argparse
import copy
import json
import math
import struct
from pathlib import Path

from hardware_screen import coil, pulse

TAU = 2 * math.pi


def lathe(profile, segments=96):
    """Profile is (radius, z, hexagonal); close it to produce a closed body."""
    rings = []
    for r, z, hexa in profile:
        ring = []
        for j in range(segments):
            theta = TAU * j / segments
            radius = (
                r / math.cos((theta + math.pi / 6) % (math.pi / 3) - math.pi / 6)
                if hexa
                else r
            )
            ring.append((radius * math.cos(theta), radius * math.sin(theta), z))
        rings.append(ring)
    triangles = []
    for i in range(len(rings)):
        a, b = rings[i], rings[(i + 1) % len(rings)]
        for j in range(segments):
            k = (j + 1) % segments
            triangles.extend([(a[j], b[j], b[k]), (a[j], b[k], a[k])])
    return orient(triangles)


def orient(triangles):
    def cross(a, b):
        return (
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        )

    def sub(a, b):
        return tuple(x - y for x, y in zip(a, b))

    def dot(a, b):
        return sum(x * y for x, y in zip(a, b))

    triangles = [
        t
        for t in triangles
        if dot(
            cross(sub(t[1], t[0]), sub(t[2], t[0])),
            cross(sub(t[1], t[0]), sub(t[2], t[0])),
        )
        > 1e-18
    ]
    volume6 = sum(dot(a, cross(b, c)) for a, b, c in triangles)
    assert abs(volume6) > 1e-9
    if volume6 < 0:
        triangles = [(a, c, b) for a, b, c in triangles]
    # Welded undirected edge counts must be two, even at axial fan centres.
    counts = {}
    key = lambda p: tuple(round(x, 7) for x in p)
    for t in triangles:
        for a, b in zip(t, t[1:] + t[:1]):
            e = tuple(sorted((key(a), key(b))))
            counts[e] = counts.get(e, 0) + 1
    assert all(n == 2 for n in counts.values()), "Generated mesh is not closed"
    return triangles


def heater(zlo, zhi, hole=3.0, hx=11.5, hy=8.0):
    angles = sorted(
        set(
            [TAU * j / 96 for j in range(96)]
            + [math.atan2(y, x) % TAU for x in [-hx, hx] for y in [-hy, hy]]
        )
    )
    inner = [(hole * math.cos(t), hole * math.sin(t)) for t in angles]
    outer = []
    for t in angles:
        r = min(hx / max(abs(math.cos(t)), 1e-12), hy / max(abs(math.sin(t)), 1e-12))
        outer.append((r * math.cos(t), r * math.sin(t)))
    rings = [
        [(x, y, z) for x, y in ring]
        for ring, z in [(outer, zhi), (outer, zlo), (inner, zlo), (inner, zhi)]
    ]
    triangles = []
    for i in range(4):
        a, b = rings[i], rings[(i + 1) % 4]
        for j in range(len(angles)):
            k = (j + 1) % len(angles)
            triangles.extend([(a[j], b[j], b[k]), (a[j], b[k], a[k])])
    return orient(triangles)


def winding(c, centre_z, segments=24):
    p = c["wire_diameter_mm"] * 1.08
    n = c["turns_per_layer"]
    span = n * p
    points = []
    for layer in range(c["layers"]):
        r = c["inner_radius_mm"] + (layer + 0.5) * p
        for j in range(n * segments + 1):
            u = j / segments
            z = centre_z + ((u / n - 0.5) * span) * (1 if layer % 2 == 0 else -1)
            points.append((r * math.cos(TAU * u), r * math.sin(TAU * u), z))
    # A return lead outside the winding closes the actual source path.
    start, end = points[0], points[-1]
    outside = c["outer_diameter_mm"] / 2 + 3.0
    below = centre_z - span / 2 - 2.0
    points.extend(
        [
            (outside, end[1], end[2]),
            (outside, 0.0, below),
            (start[0], 0.0, below),
            start,
        ]
    )
    return points


def emit(path, bodies, curve):
    binary = bytearray()
    views, accessors, meshes, nodes = [], [], [], []
    for name, points, mode in [
        (name, [p for t in ts for p in t], 4) for name, ts in bodies
    ] + ([("Series coil with return lead", curve, 3)] if curve else []):
        offset = len(binary)
        for p in points:
            binary.extend(struct.pack("<fff", *(v * 0.001 for v in p)))
        views.append(
            {"buffer": 0, "byteOffset": offset, "byteLength": len(binary) - offset}
        )
        accessors.append(
            {
                "bufferView": len(views) - 1,
                "componentType": 5126,
                "count": len(points),
                "type": "VEC3",
                "min": [min(p[k] for p in points) * 0.001 for k in range(3)],
                "max": [max(p[k] for p in points) * 0.001 for k in range(3)],
            }
        )
        meshes.append(
            {
                "name": name,
                "primitives": [
                    {"attributes": {"POSITION": len(accessors) - 1}, "mode": mode}
                ],
            }
        )
        nodes.append({"mesh": len(meshes) - 1, "name": name})
    path.with_suffix(".bin").write_bytes(binary)
    data = {
        "asset": {
            "version": "2.0",
            "generator": "MHD hardware study; E3D dimension-based simplified CAD",
        },
        "buffers": [{"uri": path.with_suffix(".bin").name, "byteLength": len(binary)}],
        "bufferViews": views,
        "accessors": accessors,
        "meshes": meshes,
        "nodes": nodes,
        "scenes": [{"nodes": list(range(len(nodes)))}],
        "scene": 0,
    }
    path.write_text(json.dumps(data, indent=2))


def make(args):
    root = Path(__file__).resolve().parents[1]
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    custom = args.family == "foil"
    if args.feed_neck and not custom:
        raise ValueError("Feed-neck fixture is currently the foil family")
    if args.heater and args.family != "v6":
        raise ValueError("Bulk heater fixture currently represents V6 only")
    length = 10.0 if custom else (21.0 if args.family == "volcano" else 12.5)
    rin, ro = args.chamber / 2, args.orifice / 2
    land = (
        args.land
        if args.land is not None
        else (
            {0.4: 0.6, 0.5: 1.0 if args.family == "volcano" else 0.9, 0.6: 1.2}[
                args.orifice
            ]
        )
    )
    coil_centre = (
        args.coil_z
        if args.coil_z is not None
        else (6.0 if custom else (10.0 if args.family == "volcano" else 3.5))
    )
    if custom:
        profile = [
            (rin + 0.5, length, False),
            (rin + 0.5, 0.0, False),
            (ro, 0.0, False),
            (ro, land, False),
            (rin, land, False),
            (rin, length, False),
        ]
        if args.feed_neck:
            profile = profile[:-1] + [
                (rin, 6.0, False),
                (0.6, 6.0, False),
                (0.6, length, False),
            ]
        cone_top = land
    else:
        shoulder, hex_top, hex_bottom = 6.5, 5.0, 2.0
        cone_top = land + (rin - ro) / math.tan(math.pi / 6)
        profile = [
            (3.0, length, False),
            (3.0, shoulder, False),
            (2.5, shoulder, False),
            (2.5, hex_top, False),
            (3.5, hex_top, True),
            (3.5, hex_bottom, True),
            (args.orifice * 1.25 + 2 * math.tan(math.radians(35)), 2.0, False),
            (args.orifice * 1.25, 0.0, False),
            (ro, 0.0, False),
            (ro, land, False),
            (rin, cone_top, False),
            (rin, length - 0.3, False),
            (rin + 0.3, length, False),
        ]
    level = (
        args.level
        if args.level is not None
        else length - (1.0 if args.feed_neck else 2.0)
    )
    liquid = [
        (0.0, level, False),
        (rin, level, False),
        (rin, cone_top, False),
        (ro, land, False),
        (ro, 0.0, False),
        (0.0, 0.0, False),
    ]
    if custom:
        liquid = [
            (0.0, level, False),
            (rin, level, False),
            (rin, land, False),
            (ro, land, False),
            (ro, 0.0, False),
            (0.0, 0.0, False),
        ]
    if args.feed_neck:
        liquid = [
            (0.4, level, False),
            (0.6, level, False),
            (0.6, 6.0, False),
            (rin, 6.0, False),
            (rin, land, False),
            (ro, land, False),
            (ro, 0.0, False),
            (0.0, 0.0, False),
            (0.0, 5.5, False),
            (0.4, 5.5, False),
        ]
    bodies = [("Stainless nozzle", lathe(profile)), ("Liquid solder", lathe(liquid))]
    if args.feed_neck:
        bodies.append(
            (
                "Fixed feed wire",
                lathe(
                    [
                        (0.0, 11.0, False),
                        (0.4, 11.0, False),
                        (0.4, 5.5, False),
                        (0.0, 5.5, False),
                    ]
                ),
            )
        )
    if args.heater:
        zlo = 6.5
        bodies.append(("Simplified aluminium heater block", heater(zlo, zlo + 11.5)))
    c = coil(
        args.coil_inner, args.coil_length, args.wire, args.layers, args.coil_temperature
    )
    drive = args.drive_voltage if args.drive_voltage is not None else args.voltage
    if not 0 < drive <= args.voltage:
        raise ValueError("Commanded pulse voltage must fit supply")
    electrical = pulse(c, drive, args.current, args.frequency, rin)
    if electrical is not None:
        electrical.update(bus_v=args.voltage, drive_voltage_v=drive)
    if electrical is None:
        raise ValueError("Requested peak is not reachable with this unloaded RL model")
    tr, tf = electrical["rise_us"] * 1e-6, electrical["fall_us"] * 1e-6
    delay = 0.0
    samples = [
        [
            delay + tr * j / 40,
            drive
            / (c["coil_r_ohm"] + 0.002)
            * (1 - math.exp(-tr * j / 40 * (c["coil_r_ohm"] + 0.002) / c["air_l_h"])),
        ]
        for j in range(41)
    ]
    samples += [
        [
            tr + tf * j / 40,
            max(
                0.0,
                (args.current + drive / (c["coil_r_ohm"] + 0.002))
                * math.exp(-tf * j / 40 * (c["coil_r_ohm"] + 0.002) / c["air_l_h"])
                - drive / (c["coil_r_ohm"] + 0.002),
            ),
        ]
        for j in range(1, 41)
    ]
    if args.freewheel:
        # Resistive current decay in a shorted bridge. Truncate only below
        # 1e-5 of peak; assess tail truncation and current-dependent R later.
        tau = c["air_l_h"] / (c["coil_r_ohm"] + 0.002)
        tf = -tau * math.log(1e-5)
        samples = samples[:41] + [
            [tr + tf * j / 160, args.current * math.exp(-tf * j / 160 / tau)]
            for j in range(1, 161)
        ]
        samples += [[tr + tf + tau / 100, 0.0]]
        hold_i2dt = args.current**2 * tau / 2
        t = [row[0] for row in samples[:41]]
        current = [row[1] for row in samples[:41]]
        rise_i2dt = sum(
            (t[i + 1] - t[i]) * (current[i] ** 2 + current[i + 1] ** 2) / 2
            for i in range(40)
        )
        electrical.update(
            fall_mode="resistive freewheel",
            fall_us=tf * 1e6,
            copper_power_w=c["coil_r_ohm"] * (rise_i2dt + hold_i2dt) * args.frequency,
            warning="Freewheel losses exclude diode/switch dynamics; explicit reaction voltage check required",
        )
    stem = args.name
    cad = out / (stem + ".gltf")
    curve = winding(c, coil_centre)
    emit(cad, bodies, curve)
    p = json.loads((root / "assets/nozzle-coil.mhd.json").read_text())
    p["geometry"] = cad.name
    p["bodies"] = [
        {
            "name": name + " / 0",
            "triangles": len(ts),
            "visible": True,
            "simulation": {
                "included": True,
                "material_id": mid,
                "temperature_c": 250.0,
                "fixed_temperature_c": None,
                "heat_source_w_m3": 0.0,
            },
        }
        for (name, ts), mid in [
            (
                item,
                {
                    "Stainless nozzle": 14,
                    "Liquid solder": 4,
                    "Simplified aluminium heater block": 8,
                    "Fixed feed wire": 15,
                }[item[0]],
            )
            for item in bodies
        ]
    ]
    wall = copy.deepcopy(p["materials"][6])
    wall.update(
        id=14,
        name="303/316 stainless screening bracket",
        conductivity_s_m=args.wall_sigma,
        relative_permeability=1.0,
        density_kg_m3=7900.0,
        notes="Room-temperature resistivity references; conductivity at 250 C is bracketed, not calibrated. Thread/fillet simplifications need refinement.",
        reference="https://www.carpentertechnology.com/hubfs/7407324/Material%20Saftey%20Data%20Sheets/303.pdf",
    )
    p["materials"].append(wall)
    if args.feed_neck:
        wire_mat = copy.deepcopy(p["materials"][4])
        wire_mat.update(
            id=15,
            name="Stationary cold feedstock screening",
            conductivity_s_m=6e6,
            dynamic_viscosity_pa_s=None,
            surface_tension_n_m=None,
            notes="Fixed feedstock position for one pulse; no replenishment or melting calculation. Electrical conductivity is a solid-solder estimate.",
        )
        p["materials"].append(wire_mat)
        p["bodies"][2]["simulation"]["temperature_c"] = 20.0
    p["materials"][4].update(
        name="Sn99.3Cu0.7 liquid screening",
        conductivity_s_m=2.2e6,
        density_kg_m3=7000.0,
        surface_tension_n_m=0.53,
        dynamic_viscosity_pa_s=0.002,
        notes="Electrical conductivity approx from Zhang2010 Fig2; density near liquid Sn and viscosity assumption. Surface/contact uncertainty must be bracketed.",
        reference="https://kovmat.sav.sk/full.php?cc=5&rr=48&ss=321",
    )
    f32 = lambda x: struct.unpack("<f", struct.pack("<f", x))[0]
    p["cad_curves"] = [
        {
            "name": "Series coil with return lead / 0:0",
            "points_mm": [
                [f32(f32(v * 0.001) * 1000) for v in point] for point in curve
            ],
        }
    ]
    p["electromagnetics"]["wires"] = [
        {
            "cad_curve": 0,
            "reverse": False,
            "name": "Series coil with return lead / 0:0",
            "core_radius_mm": args.wire / 2,
            "current": {"kind": "table", "samples": samples},
        }
    ]
    p["electromagnetics"].update(relative_tolerance=1e-4, max_iterations=4000)
    dt = tr / 20
    if args.fluid:
        dt = min(
            dt,
            0.08
            * math.sqrt(
                (7000.0 + 1.2) * (args.cell * 1e-3) ** 3 / (2 * math.pi * 0.53)
            ),
        )
    p["run"].update(
        dt_s=dt, fluid_enabled=args.fluid, thermal_enabled=False, prescribed_jet=None
    )
    p["fluid"].update(material_id=4, dt_s=dt, surface={"enabled": args.fluid})
    radius = 13.0 if args.heater else max(5.0, rin + 2.0)
    top = length + 7.5 if args.heater else length + 2.0
    p["domain"] = {
        "min_mm": [-radius, -radius, -2.0],
        "max_mm": [radius, radius, top],
        "cell_mm": args.cell,
    }
    project = out / (stem + ".mhd.json")
    project.write_text(json.dumps(p, indent=2))
    (out / (stem + "-assumptions.json")).write_text(
        json.dumps(
            {
                "geometry": "E3D dimension-based approximations, no thread helix/fillets; foil variant custom",
                "electrical": "Unloaded constant RL bridge estimate; eddy loading must be checked",
                "electrical_estimate": electrical,
                "nominal_land_mm": land,
                "liquid_level_mm": level,
                "coil_centre_z_mm": coil_centre,
                "family": args.family,
                "heater_block": args.heater,
                "wall_sigma_s_m": args.wall_sigma,
                "em_steps_through_pulse": math.ceil((tr + tf) / dt),
                "step_s": dt,
            },
            indent=2,
        )
    )
    print(project)
    print(
        json.dumps(
            {
                "turns": c["turns"],
                "rise_us": tr * 1e6,
                "fall_us": tf * 1e6,
                "dt_us": dt * 1e6,
                "B_air_T": electrical["air_b_peak_t"],
                "copper_W": electrical["copper_power_w"],
            }
        )
    )


if __name__ == "__main__":
    a = argparse.ArgumentParser(description=__doc__)
    a.add_argument("--name", required=True)
    a.add_argument("--family", choices=["v6", "volcano", "foil"], default="v6")
    a.add_argument(
        "--output", type=Path, default=Path("artifacts/hardware-research/cases")
    )
    for name, default in [
        ("chamber", 3.2),
        ("orifice", 0.4),
        ("coil-inner", 4.3),
        ("coil-length", 3.0),
        ("wire", 0.6),
        ("voltage", 48.0),
        ("current", 200.0),
        ("frequency", 200.0),
        ("coil-temperature", 100.0),
        ("cell", 0.2),
        ("wall-sigma", 1.3e6),
    ]:
        a.add_argument("--" + name, type=float, default=default)
    for name in ["coil-z", "level", "land"]:
        a.add_argument("--" + name, type=float)
    a.add_argument("--drive-voltage", type=float)
    a.add_argument("--feed-neck", action="store_true")
    a.add_argument("--freewheel", action="store_true")
    a.add_argument("--layers", type=int, default=4)
    a.add_argument("--heater", action="store_true")
    a.add_argument("--fluid", action="store_true")
    make(a.parse_args())
