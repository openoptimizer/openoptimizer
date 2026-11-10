//! Upper bound for the value of a single panel (pricing bound of the column generation).
//!
//! Solves the unconstrained guillotine knapsack (every piece kind may be used any number of times)
//! by dynamic programming over normal patterns on a coarse grid. Piece sizes are rounded down and
//! the panel size is rounded down to the grid, which can only enlarge the set of feasible
//! patterns: a set of pieces that fits side by side in reality also fits after rounding, because
//! `sum(floor(w_i / g)) <= floor(sum(w_i) / g)`. The result is therefore a valid upper bound for every real guillotine
//! pattern, and with it the column generation yields a proven lower bound for the panel area.

use super::model::*;

/// Largest DP effort accepted per panel format (states x cut positions).
const MAX_WORK: f64 = 4.0e7;
const GRID_STEPS: [i64; 6] = [900, 600, 420, 300, 210, 150];
const NONE: usize = usize::MAX;

#[derive(Debug, Clone, Copy)]
enum Choice {
    Empty,
    Piece(u32),
    Left,
    Down,
    Vertical(u32),
    Horizontal(u32),
}

/// One fitting orientation of a kind on the grid.
#[derive(Debug, Clone, Copy)]
struct GridItem {
    kind: usize,
    w: usize,
    h: usize,
}

#[derive(Debug, Clone)]
pub struct GridKnapsack {
    wg: usize,
    hg: usize,
    items: Vec<GridItem>,
}

pub struct KnapsackSolution {
    pub value: f64,
    /// Copies per kind in the best (relaxed) pattern.
    pub counts: Vec<u32>,
}

impl GridKnapsack {
    /// Builds the grid for one panel format. `kinds[k]` is a representative piece of kind `k`.
    pub fn new(spec: &BinSpec, kinds: &[&Piece]) -> Option<Self> {
        let orientations: Vec<(usize, Orientation)> = kinds
            .iter()
            .enumerate()
            .flat_map(|(k, piece)| piece.orientations(false).map(move |o| (k, o)))
            .filter(|(_, o)| o.w <= spec.w && o.h <= spec.h)
            .collect();
        if orientations.is_empty() {
            return None;
        }
        let min_w = orientations.iter().map(|(_, o)| o.w).min()?;
        let min_h = orientations.iter().map(|(_, o)| o.h).min()?;
        for steps in GRID_STEPS {
            // The grid is never coarser than the smallest piece, so no piece becomes free.
            let gx = ((spec.w + steps - 1) / steps).min(min_w).max(1);
            let gy = ((spec.h + steps - 1) / steps).min(min_h).max(1);
            let wg = (spec.w / gx) as usize;
            let hg = (spec.h / gy) as usize;
            if wg > 6000 || hg > 6000 {
                continue;
            }
            let items: Vec<GridItem> = orientations
                .iter()
                .map(|&(kind, o)| GridItem {
                    kind,
                    w: (o.w / gx) as usize,
                    h: (o.h / gy) as usize,
                })
                .collect();
            let cx = normals(items.iter().map(|i| i.w), wg).len() as f64;
            let cy = normals(items.iter().map(|i| i.h), hg).len() as f64;
            if cx * cy * (cx + cy) / 2.0 <= MAX_WORK {
                return Some(Self { wg, hg, items });
            }
        }
        None
    }

    /// Best relaxed pattern for the given values per kind (only positive values count).
    pub fn solve(&self, values: &[f64]) -> KnapsackSolution {
        let mut counts = vec![0u32; values.len()];
        let items: Vec<GridItem> = self
            .items
            .iter()
            .copied()
            .filter(|i| values[i.kind] > 0.0)
            .collect();
        if items.is_empty() {
            return KnapsackSolution { value: 0.0, counts };
        }
        let nx = normals(items.iter().map(|i| i.w), self.wg);
        let ny = normals(items.iter().map(|i| i.h), self.hg);
        let fx = floor_index(&nx, self.wg);
        let fy = floor_index(&ny, self.hg);
        let (cx, cy) = (nx.len(), ny.len());
        let mut value = vec![0.0f64; cx * cy];
        let mut choice = vec![Choice::Empty; cx * cy];
        for (n, item) in items.iter().enumerate() {
            let at = fx[item.w] * cy + fy[item.h];
            if values[item.kind] > value[at] {
                value[at] = values[item.kind];
                choice[at] = Choice::Piece(n as u32);
            }
        }
        for i in 0..cx {
            for j in 0..cy {
                let at = i * cy + j;
                let mut best = value[at];
                let mut pick = choice[at];
                if i > 0 && value[at - cy] > best {
                    best = value[at - cy];
                    pick = Choice::Left;
                }
                if j > 0 && value[at - 1] > best {
                    best = value[at - 1];
                    pick = Choice::Down;
                }
                let x = nx[i];
                for (ci, &c) in nx.iter().enumerate() {
                    if c * 2 > x {
                        break;
                    }
                    let ri = fx[x - c];
                    if ri == NONE {
                        continue;
                    }
                    let v = value[ci * cy + j] + value[ri * cy + j];
                    if v > best + 1e-12 {
                        best = v;
                        pick = Choice::Vertical(ci as u32);
                    }
                }
                let y = ny[j];
                for (cj, &c) in ny.iter().enumerate() {
                    if c * 2 > y {
                        break;
                    }
                    let rj = fy[y - c];
                    if rj == NONE {
                        continue;
                    }
                    let v = value[i * cy + cj] + value[i * cy + rj];
                    if v > best + 1e-12 {
                        best = v;
                        pick = Choice::Horizontal(cj as u32);
                    }
                }
                value[at] = best;
                choice[at] = pick;
            }
        }
        let (ti, tj) = (fx[self.wg], fy[self.hg]);
        if ti == NONE || tj == NONE {
            return KnapsackSolution { value: 0.0, counts };
        }
        let mut stack = vec![(ti, tj)];
        while let Some((i, j)) = stack.pop() {
            match choice[i * cy + j] {
                Choice::Empty => {}
                Choice::Piece(n) => counts[items[n as usize].kind] += 1,
                Choice::Left => stack.push((i - 1, j)),
                Choice::Down => stack.push((i, j - 1)),
                Choice::Vertical(ci) => {
                    let ci = ci as usize;
                    stack.push((ci, j));
                    stack.push((fx[nx[i] - nx[ci]], j));
                }
                Choice::Horizontal(cj) => {
                    let cj = cj as usize;
                    stack.push((i, cj));
                    stack.push((i, fy[ny[j] - ny[cj]]));
                }
            }
        }
        KnapsackSolution {
            value: value[ti * cy + tj],
            counts,
        }
    }
}

/// All sums of item lengths up to `limit` (normal pattern coordinates), ascending, without 0.
fn normals(lengths: impl Iterator<Item = usize>, limit: usize) -> Vec<usize> {
    let mut distinct: Vec<usize> = lengths.filter(|&l| l > 0 && l <= limit).collect();
    distinct.sort_unstable();
    distinct.dedup();
    let mut reachable = vec![false; limit + 1];
    reachable[0] = true;
    for &l in &distinct {
        for x in l..=limit {
            if reachable[x - l] {
                reachable[x] = true;
            }
        }
    }
    (1..=limit).filter(|&x| reachable[x]).collect()
}

/// For every length up to `limit` the index of the largest normal coordinate not above it.
fn floor_index(normals: &[usize], limit: usize) -> Vec<usize> {
    let mut index = vec![NONE; limit + 1];
    let mut current = NONE;
    let mut next = 0;
    for (x, slot) in index.iter_mut().enumerate() {
        while next < normals.len() && normals[next] <= x {
            current = next;
            next += 1;
        }
        *slot = current;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(w: i64, h: i64, can_rotate: bool, kind: usize) -> Piece {
        Piece {
            id: format!("p{kind}"),
            w,
            h,
            can_rotate,
            real_area: w * h,
            kind,
        }
    }

    #[test]
    fn counts_pieces_per_panel_exactly_for_strips() {
        // 4 pieces of width 25 fit across 100, a fifth does not; height allows only one row.
        let spec = BinSpec {
            type_index: 0,
            w: 10_000,
            h: 10_000,
            panel_area: 1.0,
        };
        let a = piece(2_500, 6_000, false, 0);
        let b = piece(2_600, 6_000, false, 1);
        let knapsack = GridKnapsack::new(&spec, &[&a, &b]).unwrap();
        let only_a = knapsack.solve(&[1.0, 0.0]);
        assert_eq!(only_a.value, 4.0);
        let both = knapsack.solve(&[1.0, 1.0]);
        assert_eq!(both.value, 4.0);
        let prefer_b = knapsack.solve(&[1.0, 2.0]);
        // 3 b + a = 103 does not fit, so the best pattern is 3 b = 6.
        assert_eq!(prefer_b.value, 6.0);
    }

    #[test]
    fn uses_both_cut_directions() {
        // 2 x 2 grid of squares.
        let spec = BinSpec {
            type_index: 0,
            w: 10_000,
            h: 10_000,
            panel_area: 1.0,
        };
        let a = piece(5_000, 5_000, false, 0);
        let solution = GridKnapsack::new(&spec, &[&a]).unwrap().solve(&[1.0]);
        assert_eq!(solution.value, 4.0);
        assert_eq!(solution.counts, vec![4]);
    }
}
