# Thicker radiused tip with heated contact fingers

1. This is the current mechanical prototype candidate for inspection/research.
   It retains the 10 mm M6 nozzle body, 2 mm lower bore, raised pool, 0.4 mm
   outlet and 0.6 mm outlet land. The square tip is now 0.8 mm across, with a
   0.2 mm nominal wall and an explicit R0.1 root. Copper contact height is
   0.35 mm, leaving nominal 0.05 mm clearance below the fillet start. A cutter
   with this nominal corner radius is catalogued; no machining process,
   tolerance stack, clamp preload or structural qualification is implied.
2. The matched native 50 µm straight-finger comparison needs about 108 A to
   match the thinner tip's 93 A cold outlet force. A rounded 110 A comparison
   waveform is 23.4 A RMS at an assumed 200 Hz. It is not a qualified jetting
   waveform. The separate ideal-joint one-shot thermal replay peaks near
   261 °C from 250 °C; it does not establish repetitive temperatures or real
   microscopic contact heating.
3. The full heated-contact fixture has two 6×10×0.4 mm copper plates separated
   from the heater by 0.1 mm electrical mica insulation. Its stationary native
   100 µm screen gives about 239 °C minimum liquid with a 250 °C ideal heater,
   25 °C outer wire ends and effective sheet/interface k=0.1 W/(m K). Feeding,
   exterior losses, Joule heat and phase-dependent properties are absent.
   These steady and one-shot temperatures must not be added together.
4. The full 100 µm static charge fixture is about 7.5 mΩ, including both 50 mm,
   1 mm² straight copper supply extensions. At the comparison waveform this
   is about 0.83 V resistive drop at 110 A and 4.1 W at the assumed 200 Hz.
   Those extensions are a thermal/electrical fixture, not final cable routing;
   count them only once in the driver budget. Hot copper properties are fixed.
5. The four stock-sized SmCo bars retain a 3.4 mm pole gap and nominal 2 mm
   projection below the outlet. Actual carriers/retainers are not yet drawn;
   their lowest parts must count toward stand-off. The magnetic model gives
   roughly 9.4 N attraction per pole group. A nonmagnetic metal carrier should
   carry that load without using the nozzle as a stop. Exact hot magnet grade,
   carrier temperature, polarity and field stability still need qualification.
6. Opening `hot-contact.mhd.json` is for CAD/material inspection. The GUI does
   not implement the dedicated terminal-current and thermal boundary experiments.
   No current geometry has a qualified moving-fluid/drop-rate result. See
   `hardware/m6-rooted08-screening.json` and `hardware/bench-test-plan.md`.

Generate the source tip with:

```sh
TMPDIR="$PWD/artifacts/build-temp" uv run --offline --with manifold3d --with matplotlib \
  python scripts/make_stub_m6_cases.py NEW_TIP_DIR --only stub4 \
  --pole-gap-mm 3.4 --heater-clearance-mm 1 --lower-drill-half-angle-deg 59 \
  --boss-width-mm .8 --tip-root-radius-mm .1 --contact-height-mm .35
```

`make_hot_contact_case.py` then adds the folded copper and insulation using its
`--source-case` argument, relative to `artifacts/hardware-research/cases`, and
`--anchor-width-mm 10`. Every input is research geometry, not a shop drawing.
