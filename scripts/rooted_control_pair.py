"""Name and validate the single-variable electrode controls used in this study."""

import math


def case_names(study, reference_retract=.25, candidate_retract=.5):
    prefix = "m6-rooted08-flow110-"
    suffix = "-motionabs1e-06A-h010-dt1p5"
    baseline = prefix + "r50" + suffix
    if study == "timestep":
        return [prefix + "r25-motionabs1e-06A-h010-dt3", prefix + "r25" + suffix]
    if study == "retract":
        return [prefix + f"r{r*100:g}" + suffix for r in (reference_retract, candidate_retract)]
    if study == "delay":
        return [baseline, baseline + "-delay0us"]
    if study == "interface":
        return [baseline, baseline + "-interface1e-09ohmm2"]
    raise ValueError(f"Unknown control: {study}")


def validate_pair(statuses, study, reference_retract=.25, candidate_retract=.5):
    """Reject attribution when any recorded input other than the intended one differs.

    Output diagnostics are intentionally excluded. This validates recorded native
    configuration, not historical file identity or completeness of the PDE model.
    """
    a, b = statuses
    variable = {"timestep": "timestep_s", "retract": "retract_fraction",
                "delay": "retract_delay_s", "interface": "contact_resistivity_ohm_m2"}[study]
    keys = ["project", "grid", "charge_solver", "charge_potential_representation",
            "charge_requested_relative_tolerance", "peak_A", "positive_duration_s",
            "rise_s", "retract_delay_s", "retract_fraction", "contact_resistivity_ohm_m2",
            "frequency_hz", "uniform_acceleration_m_s2", "periodic_fluid_axes",
            "terminal_boundary", "pulse_start_s", "timestep_s"]
    for key in keys:
        if key != variable and a[key] != b[key]:
            raise ValueError(f"Unexpected difference beyond {variable}: {key}")
    for key in ["motional_absolute_tolerance_A", "unit_relative_tolerance"]:
        if a["charge_boundary_criteria"][key] != b["charge_boundary_criteria"][key]:
            raise ValueError(f"Different current criteria: {key}")
    for status in statuses:
        if status["tail_timestep_s"] != status["timestep_s"]:
            raise ValueError("Expected uniform timesteps")
    expected = {
        "timestep": (3e-6, 1.5e-6),
        "retract": (reference_retract, candidate_retract),
        "delay": (25e-6, 0),
        "interface": (0, 1e-9),
    }[study]
    for actual, desired in zip((a[variable], b[variable]), expected):
        if not math.isfinite(actual) or not math.isclose(actual, desired, rel_tol=1e-7, abs_tol=1e-18):
            raise ValueError(f"Unexpected {variable}: {actual}; expected {desired}")
    if study in ("delay", "interface") and a["retract_fraction"] != .5:
        raise ValueError("Timing/interface controls require the 55 A reverse baseline")


DESCRIPTIONS = {
    "timestep": ("Completed fixed-grid timestep sensitivity; not temporal convergence",
                 "Two timesteps do not establish an asymptotic convergence rate."),
    "retract": ("Completed fixed-grid reverse-current comparison; not grid convergence",
                "Two reversal strengths at one grid do not establish an optimum or clean repeated jetting."),
    "delay": ("Completed fixed-grid retrieval-delay comparison; not grid convergence",
              "Earlier retrieval changes pulse timing, not pulse widths or reverse amplitude; one pulse does not establish recovery or repeatability."),
    "interface": ("Completed fixed-grid liquid/solid interface-resistivity sensitivity",
                  "The assumed interface resistivity applies to all conducting liquid/solid faces, including feedstock; it is not a measured Cu/SS dry-contact resistance."),
}
