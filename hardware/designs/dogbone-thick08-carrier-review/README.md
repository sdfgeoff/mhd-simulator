# Thicker replaceable strip with adapted pole carriers

1. `assembly.inspection.mhd.json` combines the 0.8 mm stainless strip, bolted copper
   contacts and adapted pole carriers. The strip keeps its top seat at z=0.5 mm,
   extending the outlet down to z=-0.3 mm. Its 0.4 mm drilled opening is 0.8 mm long.
   The earlier 0.5 mm strip's numerical force/temperature results do **not** qualify
   this thicker variant; it has not yet had an electrical, thermal or flow solve.
2. Carriers retain the poles independently of the nozzle and contact strip.
   Their envelope is 37×37.8 mm. The floor at z=-2.7 mm is **2.4 mm below this
   outlet**. That is nominal geometric projection, before assembly tolerances or
   target-surface clearance allowances.
3. The central floor opening grows to 4.2 mm to clear the lower M2 contact heads.
   Mesh distances are 0.2 mm to those fasteners, 0.5 mm to copper and heater, and
   2.89 mm to the upper nozzle. The floor's outside end edges have 0.1 mm chamfers.
   All nominal intersections are checked, with only the eight intended carrier
   screw/pilot thread envelopes permitted. Six carrier/keeper STL round trips are
   closed and connected. `strip-mm.stl` is the separate strip. All STL coordinates
   are millimetres in the assembly coordinate system; these are review inputs.
4. The simple strip-bending estimate falls from 48 to 18.75 MPa per newton of seat
   preload when thickness grows from 0.5 to 0.8 mm. This is a conditional beam
   calculation, not a hot allowable or seal qualification. The contact screws
   clamp strip to copper; they still do not provide nozzle-seat preload.
5. Seat preload, heater-anchor clamps, finish, leakage, thermal movement and actual
   cold-carriage mounting/isolation remain unresolved. Keeper lift clearance is
   nominally 0.2 mm; no magnet/carrier thermal contact or spring preload is specified.
   Tool and assembly access are not qualified. This is not a manufacturing release.
6. The 12 source bodies retain their simulation properties. The 14 added carrier,
   keeper and screw bodies are excluded from physics. This project is for inspection;
   main GUI Simulate does not configure the terminal-current research client.
   Earlier stationary head results do not qualify this complete assembly's heat paths.

Reproduce from the research checkout:

```sh
TMPDIR="$PWD/artifacts/build-temp" uv run --offline --with manifold3d --with matplotlib \
  python scripts/make_dogbone_strip_case.py artifacts/hardware-research/cases/NEW_STRIP \
  --waist-half-length-mm .4 --strip-thickness-mm .8
TMPDIR="$PWD/artifacts/build-temp" uv run --offline --with manifold3d --with matplotlib \
  python scripts/make_pole_carrier.py artifacts/hardware-research/cases/NEW_CARRIER \
  --source-project artifacts/hardware-research/cases/NEW_STRIP/assembly.mhd.json \
  --magnet-drop-mm 0 --floor-inner-mm 2.1 --floor-end-chamfer-mm .1
TMPDIR="$PWD/artifacts/build-temp" uv run --offline --with manifold3d --with matplotlib \
  python scripts/check_pole_carrier.py --case NEW_CARRIER --output NEW_CHECK.json
```

See `hardware/m6-dogbone-thick08-carrier-screening.json` and
`hardware/m6-strip-thickness-geometry-screening.json` for scope and assumptions.
