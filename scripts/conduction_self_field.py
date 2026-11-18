"""Postprocess exported cold J with a free-space Biot-Savart sum.

This diagnoses an omitted field; it does not change the GPU charge/fluid solve.
Source-cube quadrature near the target, zero self-cell integral, no external return.
Source: https://farside.ph.utexas.edu/teaching/em/lectures/node39.html
"""

import argparse
import json
from pathlib import Path

import numpy as np
from scipy.signal import fftconvolve


def field_from_current(j, h, near_subdivisions=0):
    """B = 1e-7 integral J(source) cross R / |R|^3 dV, zero-padded."""
    shape = np.array(j.shape[:3])
    z, y, x = np.meshgrid(
        *(np.arange(1 - n, n, dtype=float) * h for n in shape), indexing="ij"
    )
    r2 = x*x + y*y + z*z
    factor = np.zeros_like(r2)
    np.power(r2, -1.5, out=factor, where=r2 > 0)
    factor *= 1e-7 * h**3
    kernels = [x*factor, y*factor, z*factor]
    if near_subdivisions:
        # Integrate uniform source cubes near the target, where a point-centre
        # source is least accurate. Self-cube remains exactly zero by symmetry.
        near = (r2 > 0) & (r2 <= (4*h)**2)
        centres = np.column_stack([v[near] for v in (x, y, z)])
        offsets = (np.arange(near_subdivisions)+.5)/near_subdivisions - .5
        oz, oy, ox = np.meshgrid(offsets, offsets, offsets, indexing="ij")
        offsets = np.column_stack([ox.ravel(), oy.ravel(), oz.ravel()])*h
        r = centres[:, None, :] - offsets[None, :, :]
        integrated = np.mean(r/np.linalg.norm(r, axis=-1)[..., None]**3, axis=1)*1e-7*h**3
        for axis in range(3):
            kernels[axis][near] = integrated[:, axis]
    # Kernels own their arrays; release full coordinate grids before FFT scratch.
    del x, y, z, r2, factor
    b = np.empty_like(j, dtype=float)
    for axis in range(3):
        a, c = (axis + 1) % 3, (axis + 2) % 3
        b[..., axis] = (
            fftconvolve(j[..., a], kernels[c], mode="same")
            - fftconvolve(j[..., c], kernels[a], mode="same")
        )
    return b


def check():
    # Finite straight wire along X, observed 20 cells away on +Y.
    h, radius, current, n = .0001, .002, 10., 101
    j = np.zeros((1, 41, n, 3))
    j[0, 0, :, 0] = current / h**2
    b = field_from_current(j, h)[0, 20, n//2]
    half_length = n*h/2
    expected = 2e-7*current/radius*half_length / np.hypot(half_length, radius)
    error = abs(b[2]/expected - 1)
    assert error < 1e-4 and np.linalg.norm(b[:2]) < expected*1e-10, (b, expected)
    # Compare an off-centre FFT sample against the direct discrete sum.
    rng = np.random.default_rng(75)
    j = rng.normal(size=(5, 6, 7, 3))
    at = np.array([1, 4, 3])
    source = np.indices(j.shape[:3]).transpose(1, 2, 3, 0)
    r = (at - source)[..., ::-1] * h
    distance = np.linalg.norm(r, axis=-1)
    kernel = np.zeros_like(r)
    np.divide(r, distance[..., None]**3, out=kernel, where=distance[..., None] > 0)
    direct = np.sum(np.cross(j, kernel), axis=(0, 1, 2))*1e-7*h**3
    fft = field_from_current(j, h)[tuple(at)]
    assert np.allclose(fft, direct, rtol=1e-11, atol=1e-22), (fft, direct)
    print(json.dumps({"finite_wire_relative_error": error, "fft_direct_max_error_T": float(np.max(abs(fft-direct)))}))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("directory", type=Path, nargs="?")
    p.add_argument("--current", type=float, default=200.)
    p.add_argument("--check", action="store_true")
    p.add_argument("--near-subdivisions", type=int, choices=[0, 2, 4, 8], default=4)
    a = p.parse_args()
    if a.check:
        check()
    if a.directory is None:
        if not a.check:
            p.error("Choose an export directory or --check")
        return
    if not np.isfinite(a.current) or a.current <= 0:
        p.error("Current must be finite and positive")
    grid = json.loads((a.directory/"grid.json").read_text())
    report = json.loads((a.directory/"report.json").read_text())
    nx, ny, nz = grid["dims"]
    h = grid["cell_m"]
    f = np.memmap(a.directory/"em.bin", dtype="<f4", mode="r").reshape(nz, ny, nx, 16)
    ids = np.fromfile(a.directory/"material-ids.bin", dtype="<u4").reshape(nz, ny, nx)
    scale = a.current/report["current_A"]
    j = np.asarray(f[..., 4:7], dtype=float)*scale
    b = field_from_current(j, h, a.near_subdivisions)
    force = np.cross(j, b)
    applied_force = np.asarray(f[..., 8:11], dtype=float)*scale
    z = grid["origin_m"][2] + (np.arange(nz)+.5)*h
    regions = {}
    for name, mask in {
        "all_liquid": ids == 4,
        "liquid_outlet_land": (ids == 4) & (z[:, None, None] >= 0) & (z[:, None, None] < .0006),
    }.items():
        if not mask.any():
            continue
        bm = np.linalg.norm(b[mask], axis=-1)
        bias = np.linalg.norm(f[..., :3][mask], axis=-1)
        fm = np.linalg.norm(force[mask], axis=-1)
        fa = np.linalg.norm(applied_force[mask], axis=-1)
        regions[name] = {
            "cells": int(mask.sum()),
            "self_B_rms_T": float(np.sqrt(np.mean(bm**2))),
            "self_B_max_T": float(bm.max()),
            "applied_B_rms_T": float(np.sqrt(np.mean(bias**2))),
            "self_force_integral_N": (force[mask].sum(axis=0)*h**3).tolist(),
            "applied_force_integral_N": (applied_force[mask].sum(axis=0)*h**3).tolist(),
            "self_over_applied_force_L2": float(np.linalg.norm(fm)/np.linalg.norm(fa)),
        }
    result = {
        "source_directory": str(a.directory), "current_A": a.current,
        "method": "Free-space Biot-Savart convolution of exported cold J; zero-padded linear FFT, self-cell integral zero by symmetry. Source cubes within four cells use subcell quadrature when enabled; farther sources use cell centres.",
        "near_subdivisions": a.near_subdivisions,
        "regions": regions,
        "limits": [
            "Diagnostic only: no feedback on current, moving interface or force in the recorded fluid run.",
            "External return leads beyond exported CAD are absent; this is not a complete closed-circuit field or an error bound.",
            "Near-neighbour cell-centre quadrature requires grid refinement. Permeability is unity; magnetic diffusion and terminal transients are absent.",
            "Net force cancellation does not establish negligible local pinching or unchanged breakup.",
        ],
    }
    out = a.directory/f"self-field-{a.current:g}A-q{a.near_subdivisions}.json"
    out.write_text(json.dumps(result, indent=2)+"\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
