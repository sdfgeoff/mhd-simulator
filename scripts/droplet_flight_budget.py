"""Independent spherical-drop flight/enthalpy screen; no MHD or oxide model."""
import argparse
import json
import math
from pathlib import Path

import numpy as np
from scipy.integrate import solve_ivp
from scipy.optimize import brentq

GASES = {
    # R, cp, mu0, S_mu, k0, S_k; references at273K.
    'air': (287.05, 1007., 1.716e-5, 111., .0241, 194.),
    'nitrogen': (296.8, 1040., 1.663e-5, 107., .0242, 150.),
    'argon': (208.13, 520.3, 2.125e-5, 114., .0163, 170.),
}


def transport(gas, temperature):
    r, cp, mu0, smu, k0, sk = GASES[gas]
    factor = (temperature / 273.)**1.5
    mu = mu0 * factor * (273. + smu) / (temperature + smu)
    k = k0 * factor * (273. + sk) / (temperature + sk)
    return 101325. / (r * temperature), mu, k, cp * mu / k


def screen(diameter_mm, speed, gas, emissivity, max_step=20e-6, initial_c=250.):
    diameter = diameter_mm * .001
    rho, cp, latent, melting = 7000., 214., 59000., 227. + 273.15
    initial_t, ambient, metal_k = initial_c + 273.15, 25. + 273.15, 60.5
    energy0 = latent + cp * (initial_t - melting)
    max_re, max_bi = 0., 0.

    def temperature(energy):
        if energy > latent:
            return melting + (energy - latent) / cp
        if energy < 0:
            return melting + energy / cp
        return melting

    def derivative(_, state):
        nonlocal max_re, max_bi
        _, v, energy = state
        t = temperature(energy)
        density, mu, k, pr = transport(gas, (t + ambient) / 2.)
        re = density * abs(v) * diameter / mu
        h = (2. + .6 * math.sqrt(re) * pr**(1/3)) * k / diameter
        max_re, max_bi = max(max_re, re), max(max_bi, h*diameter/(6*metal_k))
        # Schiller-Naumann drag, regularized algebraically at v=0.
        acceleration = 9.81 * (1. - transport(gas, ambient)[0]/rho)
        acceleration -= 18*mu*v/(rho*diameter**2)*(1+.15*re**.687)
        flux = h*(t-ambient) + emissivity*5.670374419e-8*(t**4-ambient**4)
        return [v, acceleration, -6*flux/(rho*diameter)]

    def impact(_, state):
        return state[0] - .010
    impact.terminal = True
    impact.direction = 1
    solution = solve_ivp(derivative, (0., .2), [0., speed, energy0],
                         events=impact, dense_output=True, max_step=max_step,
                         rtol=1e-9, atol=[1e-12, 1e-10, 1e-5])
    if not solution.success or len(solution.t_events[0]) != 1 or max_re >= 200:
        raise RuntimeError('Flight integration failed or heat correlation Re bound exceeded')
    end = solution.t_events[0][0]
    rows = []
    for distance_mm in [.5, 2., 5., 10.]:
        t = end if distance_mm == 10. else brentq(
            lambda t: solution.sol(t)[0]-distance_mm*.001, 0., end, xtol=1e-13)
        _, v, energy = solution.sol(t)
        temp = temperature(energy)-273.15
        rows.append({'distance_mm':distance_mm,'flight_ms':t*1000,
                     'impact_speed_m_s':float(v),'temperature_c':temp,
                     'cooling_k':initial_c-temp,'liquid_fraction':float(np.clip(energy/latent,0,1))})
    return {'diameter_mm':diameter_mm,'initial_temperature_c':initial_c,'initial_speed_m_s':speed,'gas':gas,
            'emissivity':emissivity,'maximum_Re':max_re,'maximum_Biot':max_bi,'targets':rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    args = parser.parse_args()
    cases = [screen(d,v,gas,e) for d in [.4,.5,.6] for v in [.15,.4]
             for gas in GASES for e in [.1,.8]]
    refined = screen(.5,.15,'air',.8,10e-6)
    reference = next(c for c in cases if c['diameter_mm']==.5 and c['initial_speed_m_s']==.15 and c['gas']=='air' and c['emissivity']==.8)
    errors = {key:max(abs(a[key]-b[key]) for a,b in zip(reference['targets'],refined['targets']))
              for key in ['flight_ms','temperature_c','liquid_fraction']}
    assert errors['flight_ms'] < 1e-5 and errors['temperature_c'] < 1e-4
    # Sutherland reference-point and analytical gravity-only trajectory checks.
    assert abs(transport('air',273.)[1]-1.716e-5)<1e-15
    for distance in [.0005,.01]:
        v=.15; t=2*distance/(v+math.sqrt(v*v+2*9.81*distance))
        assert abs(v*t+.5*9.81*t*t-distance)<1e-14
    low_superheat = [screen(d,.15,'air',.8,initial_c=235.) for d in [.4,.5]]
    assert any(0 < row['liquid_fraction'] < 1 and abs(row['temperature_c']-227.)<1e-10
               for case in low_superheat for row in case['targets'])
    result = {'status':'Lumped spherical flight sensitivity, not a native MHD thermal result',
              'inputs':{'initial_temperature_c':250,'ambient_and_radiative_surroundings_c':25,'pressure_Pa':101325,
                        'metal_density_kg_m3':7000,'metal_cp_J_kg_K':214,'metal_latent_J_kg':59000,'melting_c':227,
                        'metal_conductivity_W_m_K':60.5,'gas_constants':GASES},
              'method':'Gravity+buoyancy+Schiller-Naumann drag; film-property Ranz-Marshall convection and greybody radiation; constant-density enthalpy with isothermal227C phase plateau.',
              'sources':{'transport':'https://doc.comsol.com/6.3/doc/com.comsol.help.cfd/cfd_ug_fluidflow_high_mach.08.43.html',
                         'correlations':'https://pmc.ncbi.nlm.nih.gov/articles/PMC6888193/',
                         'metal_inputs':'hardware/designs/v6-wide-200A/case.mhd.json; inherited screening values, not measured temperature-dependent enthalpy.'},
              'checks':{'20_to10us_max_step_absolute_changes':errors,'transport_reference_and_ballistic_identity':'pass'},
              'limits':['Not a droplet-generation or oxide-growth model; clean spherical isolated drops and still gas assumed.',
                        'Ambient gas cp and specific gas constants are constant screening inputs. Transport constants are from COMSOL tables.',
                        'Emissivity0.1/0.8 are sensitivity assumptions, not measurements of clean/oxidized solder.',
                        'No hot-nozzle/substrate view factors, gas purge velocities, evaporation, contact-line or impact/wetting physics.',
                        'Metal properties inherited SnCu screening; do not transfer these temperatures to Sn60Pb40.',
                        'Distances are droplet-centre travel after release, not guaranteed mechanical nozzle-to-substrate gaps. Mounting, breakup location and drop radius matter.'],
              'low_superheat_sensitivity':low_superheat,'cases':cases}
    with args.output.open('x') as f:json.dump(result,f,indent=2);f.write('\n')
    print(json.dumps({'checks':result['checks'],'air_half_mm':reference},indent=2))


if __name__ == '__main__':
    main()
