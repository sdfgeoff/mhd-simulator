"""Prepare tiny applied-field windows without cropping the charge solve."""
import json
from pathlib import Path
source=Path('artifacts/hardware-research/cases/m6-stub-v1/stub4.mhd.json')
for q in [3,4]:
    project=json.loads(source.read_text())
    project['domain'].update(min_mm=[-.25,-.25,0.],max_mm=[.25,.25,.6])
    for magnet in project['electromagnetics']['magnets']:
        magnet['subdivisions']=q
    with source.with_name(f'stub4-observer-q{q}.mhd.json').open('x') as out:
        json.dump(project,out,indent=2);out.write('\n')
