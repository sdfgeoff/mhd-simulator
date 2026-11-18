"""Plot completed native motion controls; no simulated field interpolation."""
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np

root = Path('artifacts/hardware-research')

def load(name):
    return json.loads((root / name).read_text())

translation = load('drop-flight-translation-periodic-h010-v1-motion-v2.json')['frames']
gravity = load('drop-flight-gravity-h010-motion.json')['frames']
fig, axes = plt.subplots(1, 3, figsize=(13, 4.6), layout='constrained')
ax = axes[0]
ax.plot([x['time_s'] * 1000 for x in translation],
        [-x['liquid_vz_m_s'] for x in translation], 'o-', label='100 µm native cells')
ax.axhline(.2, color='black', ls='--', label='Uniform translation reference')
fine_translation = load('drop-flight-translation-periodic-small-h0050-3ms-motion.json')['frames']
ax.plot([x['time_s'] * 1000 for x in fine_translation], [-x['liquid_vz_m_s'] for x in fine_translation], 'o-', label='50 µm native cells')
ax.set(title='Periodic translation at 0.2 m/s', xlabel='Time (ms)',
       ylabel='Mean downward liquid speed (m/s)')
ax.legend(frameon=False, fontsize=8)
ax = axes[1]
for cell, name in [(100, 'h010'), (50, 'h0050'), (25, 'h0025')]:
    path = root / f'drop-flight-translation-small-{name}-prefix-motion.json'
    if path.exists():
        last = json.loads(path.read_text())['frames'][-1]
        error = abs(last['velocity_error_m_s'] / last['reference_vz_m_s']) * 100
        ax.scatter(cell, error, s=45)
        ax.annotate(f'{error:.3g}%', (cell, error), xytext=(-28, 7), textcoords='offset points')
ax.set(title='Speed error after 0.6 ms', xlabel='Cell spacing (µm)',
       ylabel='Absolute relative liquid-speed error (%)', yscale='log', xlim=(15, 115), ylim=(.03, 30))
ax.set_xticks([25, 50, 100])
ax.text(.03, .95, 'Space and timestep refined together.',
        transform=ax.transAxes, fontsize=8, va='top')
ax = axes[2]
time = np.array([x['time_s'] for x in gravity])
ax.plot(time * 1000, [-x['COM_z_m'] * 1e6 for x in gravity], 'o-', label='100 µm native cells')
ax.plot(time * 1000, [-x['reference_z_m'] * 1e6 for x in gravity], '--', color='black',
        label='Dragless gravity reference')
fine_gravity = load('drop-flight-gravity-small-h0050-3ms-motion.json')['frames']
ax.plot([x['time_s'] * 1000 for x in fine_gravity], [-x['COM_z_m'] * 1e6 for x in fine_gravity], 'o-', label='50 µm native cells')
ax.set(title='Initially resting drop under gravity', xlabel='Time (ms)',
       ylabel='Downward centre travel (µm)')
ax.legend(frameon=False, fontsize=8)
for ax in axes:
    ax.grid(alpha=.2)
fig.suptitle('0.6 mm solder drop · native fluid/interface solver · no electromagnetic or thermal forcing\nCoarse/slow capillary motion is unreliable; refinement is diagnostic, not hardware qualification.', fontsize=11)
fig.savefig(root / 'diagrams/drop-motion-controls.png', dpi=170)
