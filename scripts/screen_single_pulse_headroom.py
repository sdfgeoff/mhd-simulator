"""Current-dose/circuit headroom only; no droplet or changed-waveform field solver."""

import hashlib
import json
import math
from pathlib import Path

source = Path("hardware/unipolar-coil-fast10-v2-comparison.json")
native = json.loads(source.read_text())
frequency = 200.
peak = 250.
rise_us, fall_us = 10., 100.
maximum_hold_us = (30.**2 / (peak**2 * frequency)) * 1e6 - (rise_us + fall_us) / 3
dose = peak**2 * ((rise_us + fall_us) / 3 + 30.) * 1e-6
assert abs(dose - 4.166666666666667) < 1e-12
cases = []
for case in native["cases"]:
    # Linear stationary conductivity/permeability permits amplitude scaling
    # for this SAME triangular waveform. A hold changes field history and
    # therefore must not reuse its loaded voltage/force by amplitude scaling.
    rms = case["rms_A_at_assumed200Hz"]
    voltage = case["loaded_terminal_interval_mean_voltage_range_V"]
    largest_voltage = max(map(abs, voltage))
    envelopes = []
    for rail in [24., 48.]:
        for reserve in [0., 5.]:
            available = rail - reserve
            scale = min(300. / peak, 30. / rms, available / largest_voltage)
            envelopes.append({"rail_V": rail, "chosen_reserve_V": reserve,
                              "peak_A_capped_at300": peak * scale,
                              "RMS_A_at_assumed200Hz": rms * scale,
                              "observed_stationary_voltage_range_scaled_V": [v * scale for v in voltage],
                              "force_and_heat_scale_for_same_waveform_only": scale**2})
    cases.append({"geometry": case["geometry"], "same_waveform_envelopes": envelopes})
record = {
    "status": "Analytic electrical budget; proposed plateau waveform has no native field/flow result",
    "native_triangular_source": str(source),
    "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
    "same_waveform_amplitude_envelopes": cases,
    "proposed_single_plateau_pulse": {
        "samples_us_A": [[0, 0], [10, 250], [40, 250], [140, 0]],
        "hold_us": 30., "I2dt_A2s": dose,
        "RMS_A_at_assumed200Hz": math.sqrt(dose * frequency),
        "maximum_hold_us_at250A_30Arms_assumed200Hz": maximum_hold_us,
        "rationale": "Delay the falling-edge force while preserving one nonnegative pulse and the current-dose target. Constant coil current does not maintain eddy-current force after magnetic diffusion; no sustained-pressure assumption is made.",
        "loaded_voltage": "Unknown for this changed waveform; requires native linkage/energy analysis.",
        "ejection": "Unknown; requires a matched moving-interface control."
    },
    "limits": [
        "The 300A planning cap and 5V reserve are explicit design choices, not component ratings or measured overshoot.",
        "Amplitude scaling is exact only for the stated stationary linear material model and same waveform. It does not scale moving-fluid or hot-material response.",
        "The earlier10-turn coil intersects the nominal standard heater envelope. Its voltage record does not qualify the revised12-turn package.",
        "RMS arithmetic at200Hz does not establish repetition, temperature, feed rate or a current driver's ability to generate the waveform.",
        "Negative voltage during positive current requires controlled recovery or a clamp; the supply rail alone does not describe a suitable driver.",
        "No new native test is queued by this script. Do not increase drive solely from this budget."
    ]
}
output = Path("hardware/single-pulse-headroom-screening.json")
with output.open("x") as stream:
    json.dump(record, stream, indent=2)
    stream.write("\n")
print(json.dumps(record["proposed_single_plateau_pulse"], indent=2))
