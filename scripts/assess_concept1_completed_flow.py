"""Assess the two completed magnet-size cases from existing diagnostics only."""
import csv,json
from pathlib import Path
root=Path('artifacts/hardware-research');out=Path('hardware/concept1-larger-poles-review')
cases=[]
for name in ['concept1-developed-flow-v1-driven100','concept1-larger-poles-flow-v1-driven100']:
    run=root/name;s=json.loads((run/'run-status.json').read_text());assert s['status']=='complete' and s['accepted_steps']==1200
    f=json.loads((run/'flow-analysis.json').read_text());rows=[{k:float(v) for k,v in r.items()} for r in csv.DictReader((run/'current-diagnostics.csv').open())];assert len(rows)==1200
    dose=sum(r['command_A']**2*r['timestep_s'] for r in rows)
    defect=s['lagged_electrical_work_J']-s['joule_generated_J']-s['lagged_mechanical_work_J']
    cases.append({'source':str(run),'run_status':s,'final_frame':f['frames'][-1],
                  'maximum_absolute_relative_volume_change':max(abs(x['relative_liquid_volume_change']) for x in f['frames']),
                  'any_saved_boundary_contact':any(x['liquid_touches_domain_boundary_alpha01'] for x in f['frames']),
                  'command_I2dt_A2s':dose,'RMS_A_at_assumed200Hz':(dose*200)**.5,
                  'electrical_energy_defect_J':defect,
                  'electrical_energy_defect_relative':defect/max(abs(s['lagged_electrical_work_J']),abs(s['joule_generated_J'])),
                  'field_power_defect_integral_J':sum(r['field_power_defect_W']*r['timestep_s'] for r in rows),
                  'field_power_defect_during_forward_J':sum(r['field_power_defect_W']*r['timestep_s'] for r in rows if r['time_s']<=.00025),
                  'first_saved_separated_step_by_threshold':{t:next((x['step'] for x in f['frames'] if x['connectivity_thresholds'][t]),None) for t in ['0.01','0.25','0.5','0.75']}})
record={'status':'Completed coarse-grid response comparison, not qualified jetting',
        'cases':cases,'interpretation':'Larger poles produce one separated region at all four thresholds by3ms, still nearly stationary/slightly upward at3.6ms. Original cubes retract without saved separation.',
        'current_criterion':'Unit basis10ppm; motional basis uses10ppm times absolute terminal-face flux PLUS1uA, not a hard1uA ceiling. Native accepted solves, but criteria do not establish energy accuracy.',
        'limits':['100um grid, four cells across outlet; no refinement-qualified breakup.',
                  'Electrical work identity error is comparable to mechanical work; it occurs mostly during the driven pulse. Tighten charge residual for future cases; do not claim mechanical energy qualification.',
                  'Isothermal, ideal contacts; self-field/reaction, thermal feedback and oxidation omitted.']}
(out/'completed-flow-comparison.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps([{'source':c['source'],'energy_defect_ppm':c['electrical_energy_defect_relative']*1e6,'RMS_assumed200Hz':c['RMS_A_at_assumed200Hz'],'final_detached':c['final_frame']['detached_below_tip']} for c in cases],indent=2))
