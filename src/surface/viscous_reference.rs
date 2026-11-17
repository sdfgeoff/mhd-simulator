//! Linear viscous-drop mode, independently of the grid/time integrator.
//! Lohöfer (2020), doi:10.1007/s10765-020-2608-z, equations (12)–(15).
//! The exterior is vacuum: finite ambient inertia/viscosity are not included.
use num_complex::Complex64 as C;

/// j_(l+1)(z)/j_l(z) by a backward continued fraction (Miller recurrence).
fn bessel_ratio(l: u32, z: C, depth: u32) -> C {
    let mut ratio = C::new(0., 0.);
    for k in (l + 1..=depth).rev() {
        ratio = z / (C::new((2 * k + 1) as f64, 0.) - z * ratio);
    }
    ratio
}
fn equation(s: C, omega: f64, nu: f64, radius: f64) -> C {
    let z = (s * radius.powi(2) / nu).sqrt();
    let depth = z.norm().ceil() as u32 + 80;
    let q = bessel_ratio(2, z, depth);
    let u = 2. * q / z;
    let gamma = 0.6 * u / (1. - u);
    let lamb = 5. * nu / radius.powi(2);
    s * s - 2. * lamb * s * (1. - gamma) + omega.powi(2)
}

/// Positive decay rate + i*frequency, for the oscillatory l=2 mode.
/// This benchmark helper restricts itself to the weak/moderate damping branch.
pub(super) fn eigenvalue(omega: f64, nu: f64, radius: f64) -> C {
    assert!(omega.is_finite() && omega > 0.);
    assert!(nu.is_finite() && nu > 0.);
    assert!(radius.is_finite() && radius > 0.);
    let lamb = 5. * nu / radius.powi(2);
    assert!(
        lamb < 0.5 * omega,
        "Reference branch requires Lamb decay < half the inviscid frequency"
    );
    let mut s = C::new(lamb, omega);
    for _ in 0..50 {
        let residual = equation(s, omega, nu, radius);
        let delta = omega * 1e-6;
        let derivative = (equation(s + delta, omega, nu, radius)
            - equation(s - delta, omega, nu, radius))
            / (2. * delta);
        let update = residual / derivative;
        s -= update;
        if update.norm() < omega * 1e-13 {
            assert!(s.re > 0. && s.im > 0.);
            assert!(equation(s, omega, nu, radius).norm() < 1e-11 * omega.powi(2));
            return s;
        }
    }
    panic!("Viscous-drop characteristic root did not converge");
}
#[test]
fn bessel_continued_fraction_matches_closed_half_integer_functions() {
    for z in [C::new(2., 1.), C::new(6., 6.), C::new(20., 20.)] {
        let j0 = z.sin() / z;
        let j1 = (j0 - z.cos()) / z;
        let j2 = 3. * j1 / z - j0;
        let j3 = 5. * j2 / z - j1;
        assert!((bessel_ratio(2, z, 160) - j3 / j2).norm() < 1e-12);
        assert!((bessel_ratio(2, z, 320) - bessel_ratio(2, z, 160)).norm() < 1e-13);
    }
}
#[test]
fn viscous_mode_satisfies_unrearranged_equation_and_approaches_lamb_limit() {
    let omega = (8. / (7000f64 * 0.2f64.powi(3))).sqrt();
    let mut previous_error = 1.;
    for nu in [2e-4, 2e-5, 2e-6, 2e-7] {
        let s = eigenvalue(omega, nu, 0.2);
        let z = (s * 0.04 / nu).sqrt();
        let q = bessel_ratio(2, z, z.norm().ceil() as u32 + 160);
        // Independent original characteristic equation (11), for l=2.
        let lhs = 6. / (z * z - 2. * z * q) - 1. + 4. / (z * z) * (1. - 3. * q / (z / 2. - q));
        let rhs = omega.powi(2) / (s * s);
        assert!((lhs - rhs).norm() < 1e-11);
        let error = (s.re / (5. * nu / 0.04) - 1.).abs();
        assert!(error < previous_error);
        previous_error = error;
        assert!(s.im < omega);
    }
    assert!(previous_error < 0.004);
}
