"""Brief read-only ranking of the completed 100 A geometry screens."""
import hashlib,json
from pathlib import Path
r=Path('artifacts/hardware-research');out=Path('hardware/concept1-sturdy-tip-review')
state=json.loads((r/'concept1-sturdy-v1-state.json').read_text())
assert state['stage']=='complete_needs_assessment',state['stage']
for p,h in state['hashes'].items():assert hashlib.sha256(Path(p).read_bytes()).hexdigest()==h,p
cases=[]
for label,folder in [('Current 0.4mm','concept1-larger-poles-v1-electrical100'),('Sturdy 0.4mm','concept1-sturdy-v1-040-electrical100'),('Sturdy 0.5mm','concept1-sturdy-v1-050-electrical100')]:
    p=r/folder/'report.json';v=json.loads(p.read_text())
    cases.append({'label':label,'source':str(p),'voltage_V':v['terminal_voltage_V'],
                  'Joule_W':v['joule_power_W'],'downward_liquid_force_mN':-1000*v['liquid_force_N'][2],
                  'native':v})
reference=cases[0]['downward_liquid_force_mN']
for case in cases:case['force_relative_to_current_tip']=case['downward_liquid_force_mN']/reference
result={'status':'Stationary geometry comparison only; no new jetting result',
        'cases':cases,'selection_scope':'Both new tips improve nominal wall/contact dimensions; force/current sharing guides selection, actual droplet performance still needs a driven run.',
        'limits':'Same100A hot-property/ideal-contact model and larger magnets. No dynamic thermal/contact chemistry qualification.'}
(out/'screen-results.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps([{k:v for k,v in c.items() if k!='native'} for c in cases],indent=2))
