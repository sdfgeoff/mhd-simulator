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
            radius = r / math.cos((theta + math.pi / 6) % (math.pi / 3) - math.pi / 6) if hexa else r
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
        return (a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0])
    def sub(a, b): return tuple(x-y for x, y in zip(a, b))
    def dot(a, b): return sum(x*y for x, y in zip(a, b))
    triangles = [t for t in triangles if dot(cross(sub(t[1], t[0]), sub(t[2], t[0])), cross(sub(t[1], t[0]), sub(t[2], t[0]))) > 1e-18]
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


def heater(zlo, zhi, hole=3., hx=11.5, hy=8.):
    angles = sorted(set([TAU*j/96 for j in range(96)] + [math.atan2(y, x) % TAU for x in [-hx, hx] for y in [-hy, hy]]))
    inner = [(hole*math.cos(t), hole*math.sin(t)) for t in angles]
    outer = []
    for t in angles:
        r = min(hx / max(abs(math.cos(t)), 1e-12), hy / max(abs(math.sin(t)), 1e-12))
        outer.append((r*math.cos(t), r*math.sin(t)))
    rings = [[(x, y, z) for x, y in ring] for ring, z in [(outer, zhi), (outer, zlo), (inner, zlo), (inner, zhi)]]
    triangles = []
    for i in range(4):
        a, b = rings[i], rings[(i+1) % 4]
        for j in range(len(angles)):
            k = (j+1) % len(angles)
            triangles.extend([(a[j], b[j], b[k]), (a[j], b[k], a[k])])
    return orient(triangles)


def winding(c, centre_z, segments=24):
    p = c["wire_diameter_mm"] * 1.08
    n = c["turns_per_layer"]
    span = n * p
    points = []
    for layer in range(c["layers"]):
        r = c["inner_radius_mm"] + (layer + .5)*p
        for j in range(n*segments+1):
            u = j/segments
            z = centre_z + ((u/n - .5) * span) * (1 if layer % 2 == 0 else -1)
            points.append((r*math.cos(TAU*u), r*math.sin(TAU*u), z))
    # A return lead outside the winding closes the actual source path.
    start, end = points[0], points[-1]
    outside = c["outer_diameter_mm"]/2 + 3.
    below = centre_z - span/2 - 2.
    points.extend([(outside, end[1], end[2]), (outside, 0., below), (start[0], 0., below), start])
    return points


def emit(path, bodies, curve):
    binary = bytearray()
    views, accessors, meshes, nodes = [], [], [], []
    for name, points, mode in [(name, [p for t in ts for p in t], 4) for name, ts in bodies] + [("Series coil with return lead", curve, 3)]:
        offset = len(binary)
        for p in points:
            binary.extend(struct.pack('<fff', *(v * .001 for v in p)))
        views.append({"buffer": 0, "byteOffset": offset, "byteLength": len(binary)-offset})
        accessors.append({"bufferView": len(views)-1, "componentType": 5126, "count": len(points), "type": "VEC3", "min": [min(p[k] for p in points)*.001 for k in range(3)], "max": [max(p[k] for p in points)*.001 for k in range(3)]})
        meshes.append({"name": name, "primitives": [{"attributes": {"POSITION": len(accessors)-1}, "mode": mode}]})
        nodes.append({"mesh": len(meshes)-1, "name": name})
    path.with_suffix('.bin').write_bytes(binary)
    data = {"asset": {"version": "2.0", "generator": "MHD hardware study; E3D dimension-based simplified CAD"}, "buffers": [{"uri": path.with_suffix('.bin').name, "byteLength": len(binary)}], "bufferViews": views, "accessors": accessors, "meshes": meshes, "nodes": nodes, "scenes": [{"nodes": list(range(len(nodes)))}], "scene": 0}
    path.write_text(json.dumps(data, indent=2))


def make(args):
    root = Path(__file__).resolve().parents[1]
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    custom = args.family == 'foil'
    length = 10. if custom else (21. if args.family == 'volcano' else 12.5)
    rin, ro = args.chamber/2, args.orifice/2
    land = args.land if args.land is not None else ({.4: .6, .5: .9, .6: 1.2}[args.orifice])
    coil_centre = args.coil_z if args.coil_z is not None else (6. if custom else (10. if args.family == 'volcano' else 3.5))
    if custom:
        profile = [(rin+.5, length, False), (rin+.5, 0., False), (ro, 0., False), (ro, land, False), (rin, land, False), (rin, length, False)]
        cone_top = land
    else:
        shoulder, hex_top, hex_bottom = 6.5, 5., 2.
        cone_top = land + (rin-ro)/math.tan(math.pi/6)
        profile = [(3., length, False), (3., shoulder, False), (2.5, shoulder, False), (2.5, hex_top, False), (3.5, hex_top, True), (3.5, hex_bottom, True), (args.orifice*1.25+2*math.tan(math.radians(35)), 2., False), (args.orifice*1.25, 0., False), (ro, 0., False), (ro, land, False), (rin, cone_top, False), (rin, length-.3, False), (rin+.3, length, False)]
    level = args.level if args.level is not None else length-2.
    liquid = [(0., level, False), (rin, level, False), (rin, cone_top, False), (ro, land, False), (ro, 0., False), (0., 0., False)]
    if custom:
        liquid = [(0., level, False), (rin, level, False), (rin, land, False), (ro, land, False), (ro, 0., False), (0., 0., False)]
    bodies = [("Stainless nozzle", lathe(profile)), ("Liquid solder", lathe(liquid))]
    if args.heater:
        zlo = 6.5
        bodies.append(("Simplified aluminium heater block", heater(zlo, zlo+11.5)))
    c = coil(args.coil_inner, args.coil_length, args.wire, args.layers, args.coil_temperature)
    electrical = pulse(c, args.voltage, args.current, args.frequency, rin)
    if electrical is None: raise ValueError('Requested peak is not reachable with this unloaded RL model')
    tr, tf = electrical['rise_us']*1e-6, electrical['fall_us']*1e-6
    delay = 0.
    samples = [[delay+tr*j/40, args.voltage/(c['coil_r_ohm']+.002)*(1-math.exp(-tr*j/40*(c['coil_r_ohm']+.002)/c['air_l_h']))] for j in range(41)]
    samples += [[tr+tf*j/40, max(0., (args.current+args.voltage/(c['coil_r_ohm']+.002))*math.exp(-tf*j/40*(c['coil_r_ohm']+.002)/c['air_l_h'])-args.voltage/(c['coil_r_ohm']+.002))] for j in range(1,41)]
    stem = args.name
    cad = out/(stem+'.gltf')
    curve = winding(c, coil_centre)
    emit(cad, bodies, curve)
    p = json.loads((root/'assets/nozzle-coil.mhd.json').read_text())
    p['geometry'] = cad.name
    p['bodies'] = [{"name": name+' / 0', "triangles": len(ts), "visible": True, "simulation": {"included": True, "material_id": mid, "temperature_c": 250., "fixed_temperature_c": None, "heat_source_w_m3": 0.}} for (name, ts), mid in zip(bodies, [14, 4, 8])]
    wall = copy.deepcopy(p['materials'][6]);wall.update(id=14, name='303/316 stainless screening bracket', conductivity_s_m=args.wall_sigma, relative_permeability=1., density_kg_m3=7900., notes='Room-temperature resistivity references; conductivity at 250 C is bracketed, not calibrated. Thread/fillet simplifications need refinement.', reference='https://www.carpentertechnology.com/hubfs/7407324/Material%20Saftey%20Data%20Sheets/303.pdf')
    p['materials'].append(wall)
    p['materials'][4].update(name='Sn99.3Cu0.7 liquid screening',conductivity_s_m=2.2e6,density_kg_m3=7000.,surface_tension_n_m=.53,dynamic_viscosity_pa_s=.002,notes='Electrical conductivity approx from Zhang2010 Fig2; density near liquid Sn and viscosity assumption. Surface/contact uncertainty must be bracketed.',reference='https://kovmat.sav.sk/full.php?cc=5&rr=48&ss=321')
    f32 = lambda x: struct.unpack('<f', struct.pack('<f', x))[0]
    p['cad_curves'] = [{'name':'Series coil with return lead / 0:0','points_mm':[[f32(f32(v*.001)*1000) for v in point] for point in curve]}]
    p['electromagnetics']['wires'] = [{"cad_curve":0,"reverse":False,"name":"Series coil with return lead / 0:0","core_radius_mm":args.wire/2,"current":{"kind":"table","samples":samples}}]
    p['electromagnetics'].update(relative_tolerance=1e-4,max_iterations=4000)
    dt = tr/20
    if args.fluid:
        dt = min(dt, .08*math.sqrt((7000.+1.2)*(args.cell*1e-3)**3/(2*math.pi*.53)))
    p['run'].update(dt_s=dt,fluid_enabled=args.fluid,thermal_enabled=False,prescribed_jet=None)
    p['fluid'].update(material_id=4,dt_s=dt,surface={'enabled':args.fluid})
    radius = 13. if args.heater else max(5., rin+2.)
    top = length+7.5 if args.heater else length+2.
    p['domain'] = {'min_mm':[-radius,-radius,-2.], 'max_mm':[radius,radius,top], 'cell_mm':args.cell}
    project = out/(stem+'.mhd.json');project.write_text(json.dumps(p,indent=2))
    (out/(stem+'-assumptions.json')).write_text(json.dumps({'geometry':'E3D dimension-based approximations, no thread helix/fillets; foil variant custom', 'electrical':'Unloaded constant RL bridge estimate; eddy loading must be checked', 'electrical_estimate':electrical, 'nominal_land_mm':land,'liquid_level_mm':level,'coil_centre_z_mm':coil_centre,'family':args.family,'heater_block':args.heater,'wall_sigma_s_m':args.wall_sigma,'em_steps_through_pulse':math.ceil((tr+tf)/dt),'step_s':dt},indent=2))
    print(project)
    print(json.dumps({'turns':c['turns'],'rise_us':tr*1e6,'fall_us':tf*1e6,'dt_us':dt*1e6,'B_air_T':electrical['air_b_peak_t'],'copper_W':electrical['copper_power_w']}))


if __name__ == '__main__':
    a = argparse.ArgumentParser(description=__doc__)
    a.add_argument('--name',required=True);a.add_argument('--family',choices=['v6','volcano','foil'],default='v6')
    a.add_argument('--output',type=Path,default=Path('artifacts/hardware-research/cases'))
    for name,default in [('chamber',3.2),('orifice',.4),('coil-inner',4.3),('coil-length',3.),('wire',.6),('voltage',48.),('current',200.),('frequency',200.),('coil-temperature',100.),('cell',.2),('wall-sigma',1.3e6)]:
        a.add_argument('--'+name,type=float,default=default)
    for name in ['coil-z','level','land']:a.add_argument('--'+name,type=float)
    a.add_argument('--layers',type=int,default=4);a.add_argument('--heater',action='store_true');a.add_argument('--fluid',action='store_true')
    make(a.parse_args())
