//! Estimate a dominant damped shape mode; this is measurement, not a fluid solver.
#[derive(Debug)]
pub(super) struct ModeFit {
    pub decay: f64,
    pub omega: f64,
    pub rms: f64,
}
fn prediction(p: &[f64; 5], t: f64) -> (f64, [f64; 5]) {
    let [a, b, c, decay, omega] = *p;
    let e = (-decay * t).exp();
    let (s, co) = (omega * t).sin_cos();
    let oscillation = e * (a * co + b * s);
    (
        c + oscillation,
        [
            e * co,
            e * s,
            1.,
            -t * oscillation,
            t * e * (-a * s + b * co),
        ],
    )
}
fn solve(mut matrix: [[f64; 6]; 5]) -> [f64; 5] {
    for i in 0..5 {
        let pivot = (i..5)
            .max_by(|&a, &b| matrix[a][i].abs().total_cmp(&matrix[b][i].abs()))
            .unwrap();
        matrix.swap(i, pivot);
        let d = matrix[i][i];
        assert!(d.is_finite() && d.abs() > 1e-16, "Singular shape-mode fit");
        for j in i..6 {
            matrix[i][j] /= d;
        }
        for k in 0..5 {
            if k != i {
                let d = matrix[k][i];
                for j in i..6 {
                    matrix[k][j] -= d * matrix[i][j];
                }
            }
        }
    }
    std::array::from_fn(|i| matrix[i][5])
}
pub(super) fn fit(
    trace: &[(f64, f64)],
    start: f64,
    initial_decay: f64,
    initial_omega: f64,
) -> ModeFit {
    let data: Vec<_> = trace.iter().copied().filter(|(t, _)| *t >= start).collect();
    assert!(data.len() >= 20);
    let scale = trace[0].1.abs();
    assert!(scale > 0.);
    let data: Vec<_> = data.into_iter().map(|(t, y)| (t, y / scale)).collect();
    let mut p = [1., 0., 0., initial_decay, initial_omega];
    let cost = |p: &[f64; 5]| {
        data.iter()
            .map(|&(t, y)| (prediction(p, t).0 - y).powi(2))
            .sum::<f64>()
    };
    for _ in 0..80 {
        let mut normal = [[0.; 6]; 5];
        for &(t, y) in &data {
            let (pred, j) = prediction(&p, t);
            for a in 0..5 {
                for b in 0..5 {
                    normal[a][b] += j[a] * j[b];
                }
                normal[a][5] += j[a] * (pred - y);
            }
        }
        let delta = solve(normal);
        let old = cost(&p);
        let mut accepted = false;
        for halves in 0..20 {
            let weight = 2f64.powi(-halves);
            let next = std::array::from_fn(|i| p[i] - weight * delta[i]);
            if next[3] >= 0. && next[4] > 0. && cost(&next) < old {
                p = next;
                accepted = true;
                break;
            }
        }
        if !accepted || delta.iter().all(|x| x.abs() < 1e-12) {
            break;
        }
    }
    ModeFit {
        decay: p[3],
        omega: p[4],
        rms: (cost(&p) / data.len() as f64).sqrt(),
    }
}
#[test]
fn fitted_decay_and_frequency_recover_phase_and_offset_over_different_windows() {
    let p = [0.8, -0.2, 0.13, 0.022, 0.375];
    let trace: Vec<_> = (0..2001)
        .map(|i| {
            let t = i as f64 * 0.02;
            (t, prediction(&p, t).0)
        })
        .collect();
    for start in [0., 5., 10.] {
        let measured = fit(&trace, start, 0.025, 0.38);
        assert!((measured.decay - p[3]).abs() < 1e-10);
        assert!((measured.omega - p[4]).abs() < 1e-10);
        assert!(measured.rms < 1e-10);
    }
}
