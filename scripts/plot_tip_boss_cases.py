"""Draw actual machined CAD sections alongside the coarse cold comparison."""
import json
import argparse
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Patch
from draw_hardware_layouts import load, section, COLORS

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--record', type=Path, default=Path('hardware/tip-boss-cold-screening.json'))
parser.add_argument('--extra-record', type=Path, help='Intermediate boss record, replacing rectangular case')
parser.add_argument('--output', default='tip-boss-cold-comparison.png')
args = parser.parse_args()
root = Path('artifacts/hardware-research')
record = json.loads(args.record.read_text())
if args.extra_record:
    record['cases'] = [c for c in record['cases'] if c['label'] != 'rect08x06']
    record['cases'].insert(2, json.loads(args.extra_record.read_text())['cases'][0])
fig, axes = plt.subplots(2, 4, figsize=(13, 7))
fig.subplots_adjust(top=.79, bottom=.18, left=.065, right=.98, hspace=.28, wspace=.27)
for column, case in enumerate(record['cases']):
    project = Path(case['project'])
    geometry = (project.parent / json.loads(project.read_text())['geometry']).resolve()
    parts, _ = load(str(geometry.relative_to((root/'cases').resolve()).with_suffix('')))
    title = {'control':'Current two-flat tip', 'square08':'0.8 × 0.8 mm boss',
             'rect08x06':'0.8 × 0.6 mm boss', 'square06':'0.6 × 0.6 mm boss', 'square07':'0.7 × 0.7 mm boss'}[case['label']]
    for row, plane, height in [(0, 'xy', .3), (1, 'xz', 0)]:
        ax = axes[row, column]
        section(ax, parts[:4], plane, height)
        if row == 0:
            ax.set(xlim=(-.95,.95), ylim=(-.95,.95), xlabel='x (mm)', ylabel='y (mm)')
            ax.set_title(f"{title}\nLand force {case['outlet_land_downward_force_N']*1000:.2f} mN · R {case['resistance_ohm']*1000:.2f} mΩ", fontsize=10)
        else:
            ax.set(xlim=(-.95,.95), ylim=(-.15,1.35), xlabel='x (mm)', ylabel='z (mm)')
            ax.axhline(0, color='black', ls=':', lw=.6)
        ax.set_aspect('equal')
        ax.tick_params(labelsize=8)
fig.suptitle('Removing current-bypass steel around the outlet\nActual CAD: 0.4 mm bore, same pool and near-flush magnets; external contacts retained', fontsize=14)
fig.legend(handles=[Patch(facecolor=COLORS[k], label=label) for k,label in
                    [('steel','Stainless nozzle'),('liquid','Solder'),('copper','Copper contacts')]],
           loc='lower center', bbox_to_anchor=(.5,.07), ncol=3, frameon=False)
spacing = record['cases'][0]['cell_um']
fig.text(.5,.045,f'Cold 30 A screen at {spacing:.0f} µm cells. Land force integrates liquid J×B over z=0–0.6 mm; not a pressure or jetting prediction.', ha='center', fontsize=9)
fig.text(.5,.017,f'0.1 mm walls have {100/spacing:.0f} cells. Fixed properties and ideal contacts; no current self-field, jetting or mechanical qualification.', ha='center', fontsize=9)
fig.savefig(root/'diagrams'/args.output, dpi=180)
