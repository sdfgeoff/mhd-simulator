"""Compare completed cold GPU observations at matched clocks and drive settings."""
import argparse
import csv
import hashlib
import json
from pathlib import Path

import numpy as np


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('first', type=Path)
    p.add_argument('second', type=Path)
    p.add_argument('--first-project', type=Path, required=True)
    p.add_argument('--second-project', type=Path, required=True)
    p.add_argument('--start-us', type=float, default=80.)
    p.add_argument('--end-us', type=float, default=120.)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    if not 0 <= a.start_us < a.end_us:
        raise ValueError('Invalid averaging window')
    projects = [json.loads(path.read_text()) for path in [a.first_project,a.second_project]]
    for key in ['cad_curves','electromagnetics','materials','domain','run','fluid']:
        if projects[0][key] != projects[1][key]:
            raise ValueError(f'Unmatched project setting: {key}')
    runs = []
    snapshots = []
    for directory, project_path, project in zip(
        [a.first,a.second], [a.first_project,a.second_project], projects
    ):
        status = json.loads((directory/'run-status.json').read_text())
        if status['status'] != 'complete':
            raise ValueError('Both cold runs must be complete')
        initial_path = directory/'frame-00000000/metadata.json'
        final_path = directory/f"frame-{status['accepted_steps']:08}/metadata.json"
        initial = json.loads(initial_path.read_text())
        final = json.loads(final_path.read_text())
        expected_geometry = (project_path.parent/project['geometry']).resolve()
        if Path(initial['geometry']).resolve() != expected_geometry:
            raise ValueError('Given project does not identify the actual exported geometry')
        keys = ['cad_curves','electromagnetics','materials','run','fluid','dims','origin_m','cell_m']
        for key in keys:
            if initial[key] != final[key]:
                raise ValueError(f'Cold run settings changed: {key}')
        snapshots.append({key:initial[key] for key in keys})
        rows = list(csv.DictReader((directory/'diagnostics.csv').open()))
        t = np.array([float(row['time_s']) for row in rows])
        if np.any(np.diff(t)<=0) or t[-1] < a.end_us*1e-6-1e-12:
            raise ValueError('Requested complete clock window unavailable')
        if len(rows) != status['accepted_steps']+1:
            raise ValueError('Accepted step count disagrees with CSV')
        selected = (t>=a.start_us*1e-6-1e-12)&(t<=a.end_us*1e-6+1e-12)
        window=t[selected]
        if abs(window[0]-a.start_us*1e-6)>1e-12 or abs(window[-1]-a.end_us*1e-6)>1e-12:
            raise ValueError('Averaging endpoints must be accepted sample times')
        metrics={}
        for key in ['roi_force_z_N','liquid_joule_power_W','solid_joule_power_W','radial_pressure_proxy_Pa']:
            values=np.array([float(row[key]) for row in rows])[selected]
            if not np.all(np.isfinite(values)):
                raise ValueError('Nonfinite observation')
            metrics['mean_'+key]=float(np.trapezoid(values,window)/(window[-1]-window[0]))
        values=np.array([float(row['roi_mean_Bz_T']) for row in rows])[selected]
        metrics['temporal_rms_of_spatial_mean_Bz_T']=float(np.sqrt(np.trapezoid(values**2,window)/(window[-1]-window[0])))
        current=np.array([float(row['coil_current_A']) for row in rows])
        runs.append({'directory':str(directory),'status':status,'metrics':metrics,'time':t,'current':current,
                     'diagnostics_sha256':hashlib.sha256((directory/'diagnostics.csv').read_bytes()).hexdigest(),
                     'initial_metadata_sha256':hashlib.sha256(initial_path.read_bytes()).hexdigest(),
                     'final_metadata_sha256':hashlib.sha256(final_path.read_bytes()).hexdigest(),
                     'project':str(project_path),'project_sha256':hashlib.sha256(project_path.read_bytes()).hexdigest()})
    if not np.array_equal(runs[0]['time'],runs[1]['time']) or not np.array_equal(runs[0]['current'],runs[1]['current']):
        raise ValueError('Accepted clocks/current commands differ')
    for key in ['origin_m','dims','cell_m','ROI_z_m']:
        if runs[0]['status'][key] != runs[1]['status'][key]:
            raise ValueError(f'Unmatched observation grid/ROI: {key}')
    if snapshots[0] != snapshots[1]:
        raise ValueError('Actual exported source/material/solver/grid settings differ')
    difference={key:runs[1]['metrics'][key]-runs[0]['metrics'][key] for key in runs[0]['metrics']}
    for run in runs:
        del run['time'];del run['current']
    result={'actual_initial_and_final_metadata_settings_match':True,'window_us':[a.start_us,a.end_us],'runs':runs,'second_minus_first':difference,
            'scope':'Completed cold native EM observations; same source/current/settings but intentionally different internal geometry and liquid volume. ROI force is not outlet pressure or a droplet prediction. B metric is temporal RMS of spatial mean, not cellwise RMS. No circuit or sustained-rate qualification.'}
    with a.output.open('x') as f:json.dump(result,f,indent=2);f.write('\n')
    print(json.dumps(result,indent=2))


if __name__ == '__main__':
    main()
