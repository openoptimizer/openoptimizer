//! Small dense revised simplex for covering LPs: `min c*x  s.t.  A*x >= d, x >= 0`.
//!
//! The master problem of the column generation has one row per piece kind, so a dense basis
//! inverse is cheap. The caller provides one unit column per row (a single piece on a panel), which
//! is always a feasible starting basis, so no phase one is needed. The optimal dual values are the
//! prices of the piece kinds that drive the pattern generation.

/// Column of the covering LP: sparse non-negative entries per row and a positive cost.
#[derive(Debug, Clone)]
pub struct LpCol {
    pub entries: Vec<(usize, f64)>,
    pub cost: f64,
}

#[derive(Debug, Clone)]
pub struct LpSolution {
    /// Value of every column.
    pub x: Vec<f64>,
    /// Dual value (price) of every row, never negative.
    pub duals: Vec<f64>,
    #[allow(dead_code)]
    pub objective: f64,
}

const TOL: f64 = 1e-9;
const REFACTOR_EVERY: usize = 40;
const DEGENERATE_LIMIT: usize = 40;

/// Solves the covering LP. `start[i]` must be the index of a column that is exactly the unit
/// vector of row `i`. Returns `None` only on numerical failure.
pub fn solve_cover(demand: &[f64], cols: &[LpCol], start: &[usize]) -> Option<LpSolution> {
    let m = demand.len();
    let n = cols.len();
    if m == 0 {
        return Some(LpSolution {
            x: vec![0.0; n],
            duals: Vec::new(),
            objective: 0.0,
        });
    }
    debug_assert_eq!(start.len(), m);
    // Costs are scaled to at most 1 for stable tolerances.
    let scale = cols.iter().map(|c| c.cost).fold(0.0, f64::max).max(1e-12);
    // Variables 0..n are columns, n..n+m are surplus variables (column -e_k, cost 0).
    let cost = |j: usize| if j < n { cols[j].cost / scale } else { 0.0 };

    let mut basic: Vec<usize> = start.to_vec();
    let mut is_basic = vec![false; n + m];
    for &b in &basic {
        is_basic[b] = true;
    }
    let mut binv = vec![0.0; m * m];
    for i in 0..m {
        binv[i * m + i] = 1.0;
    }
    let mut xb = demand.to_vec();
    let mut pi = vec![0.0; m];
    let mut u = vec![0.0; m];
    let mut pivot_row = vec![0.0; m];
    let mut degenerate = 0;
    let mut since_refactor = 0;
    let max_iterations = 50 * (n + m) + 1000;

    for _ in 0..max_iterations {
        pi.iter_mut().for_each(|v| *v = 0.0);
        for (i, &b) in basic.iter().enumerate() {
            let cb = cost(b);
            if cb != 0.0 {
                for (k, v) in binv[i * m..(i + 1) * m].iter().enumerate() {
                    pi[k] += cb * v;
                }
            }
        }

        let bland = degenerate > DEGENERATE_LIMIT;
        let mut enter = None;
        let mut best = -TOL;
        for j in 0..n + m {
            if is_basic[j] {
                continue;
            }
            let rc = if j < n {
                cost(j) - cols[j].entries.iter().map(|&(k, a)| pi[k] * a).sum::<f64>()
            } else {
                pi[j - n]
            };
            if rc < best {
                enter = Some(j);
                if bland {
                    break;
                }
                best = rc;
            }
        }

        let Some(j) = enter else {
            if since_refactor > 0 {
                // Confirm optimality with a fresh inverse.
                refactor(&basic, cols, n, m, demand, &mut binv, &mut xb)?;
                since_refactor = 0;
                continue;
            }
            let mut x = vec![0.0; n];
            for (i, &b) in basic.iter().enumerate() {
                if b < n {
                    x[b] = xb[i].max(0.0);
                }
            }
            let objective = x.iter().zip(cols).map(|(v, c)| v * c.cost).sum();
            return Some(LpSolution {
                x,
                duals: pi.iter().map(|v| (v * scale).max(0.0)).collect(),
                objective,
            });
        };

        u.iter_mut().for_each(|v| *v = 0.0);
        if j < n {
            for &(k, a) in &cols[j].entries {
                for (i, ui) in u.iter_mut().enumerate() {
                    *ui += binv[i * m + k] * a;
                }
            }
        } else {
            for (i, ui) in u.iter_mut().enumerate() {
                *ui = -binv[i * m + (j - n)];
            }
        }

        let mut leave: Option<usize> = None;
        let mut best_ratio = f64::INFINITY;
        for i in 0..m {
            if u[i] <= 1e-9 {
                continue;
            }
            let ratio = xb[i].max(0.0) / u[i];
            let better = match leave {
                None => true,
                Some(l) => {
                    if ratio < best_ratio - 1e-12 {
                        true
                    } else if ratio <= best_ratio + 1e-12 {
                        if bland {
                            basic[i] < basic[l]
                        } else {
                            u[i] > u[l]
                        }
                    } else {
                        false
                    }
                }
            };
            if better {
                leave = Some(i);
                best_ratio = ratio;
            }
        }
        let r = leave?; // unbounded cannot happen with positive costs

        let theta = best_ratio;
        for i in 0..m {
            xb[i] -= theta * u[i];
        }
        xb[r] = theta;
        let piv = u[r];
        for k in 0..m {
            binv[r * m + k] /= piv;
            pivot_row[k] = binv[r * m + k];
        }
        for i in 0..m {
            if i == r || u[i] == 0.0 {
                continue;
            }
            let f = u[i];
            for (k, p) in pivot_row.iter().enumerate() {
                binv[i * m + k] -= f * p;
            }
        }
        is_basic[basic[r]] = false;
        basic[r] = j;
        is_basic[j] = true;

        degenerate = if theta <= 1e-12 { degenerate + 1 } else { 0 };
        since_refactor += 1;
        if since_refactor >= REFACTOR_EVERY {
            refactor(&basic, cols, n, m, demand, &mut binv, &mut xb)?;
            since_refactor = 0;
        }
    }
    None
}

/// Recomputes the basis inverse with Gauss-Jordan elimination and the basic solution.
fn refactor(
    basic: &[usize],
    cols: &[LpCol],
    n: usize,
    m: usize,
    demand: &[f64],
    binv: &mut [f64],
    xb: &mut [f64],
) -> Option<()> {
    // Augmented matrix [B | I].
    let width = 2 * m;
    let mut a = vec![0.0; m * width];
    for (i, &b) in basic.iter().enumerate() {
        if b < n {
            for &(k, v) in &cols[b].entries {
                a[k * width + i] = v;
            }
        } else {
            a[(b - n) * width + i] = -1.0;
        }
    }
    for k in 0..m {
        a[k * width + m + k] = 1.0;
    }
    for c in 0..m {
        let p = (c..m).max_by(|&x, &y| {
            a[x * width + c]
                .abs()
                .partial_cmp(&a[y * width + c].abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
        if a[p * width + c].abs() < 1e-12 {
            return None;
        }
        if p != c {
            for k in 0..width {
                a.swap(p * width + k, c * width + k);
            }
        }
        let piv = a[c * width + c];
        for k in 0..width {
            a[c * width + k] /= piv;
        }
        for row in 0..m {
            if row == c {
                continue;
            }
            let f = a[row * width + c];
            if f != 0.0 {
                for k in 0..width {
                    a[row * width + k] -= f * a[c * width + k];
                }
            }
        }
    }
    for i in 0..m {
        binv[i * m..(i + 1) * m].copy_from_slice(&a[i * width + m..(i + 1) * width]);
        let v: f64 = (0..m).map(|k| binv[i * m + k] * demand[k]).sum();
        xb[i] = if v.abs() < 1e-11 { 0.0 } else { v };
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(entries: &[(usize, f64)], cost: f64) -> LpCol {
        LpCol {
            entries: entries.to_vec(),
            cost,
        }
    }

    #[test]
    fn solves_cutting_stock_lp() {
        // Roll 10, pieces 3 (demand 4) and 4 (demand 3): patterns [3,0], [2,1], [0,2] plus unit
        // columns. Optimum 2.5 (x[2,1] = 2, x[0,2] = 0.5), duals (0.25, 0.5).
        let cols = vec![
            col(&[(0, 1.0)], 1.0),
            col(&[(1, 1.0)], 1.0),
            col(&[(0, 3.0)], 1.0),
            col(&[(0, 2.0), (1, 1.0)], 1.0),
            col(&[(1, 2.0)], 1.0),
        ];
        let sol = solve_cover(&[4.0, 3.0], &cols, &[0, 1]).unwrap();
        assert!((sol.objective - 2.5).abs() < 1e-9, "{}", sol.objective);
        // Duals are feasible: every column has non-negative reduced cost.
        for c in &cols {
            let value: f64 = c.entries.iter().map(|&(k, a)| sol.duals[k] * a).sum();
            assert!(value <= c.cost + 1e-9);
        }
        let dual_objective: f64 = sol.duals[0] * 4.0 + sol.duals[1] * 3.0;
        assert!((dual_objective - 2.5).abs() < 1e-9);
    }
}
