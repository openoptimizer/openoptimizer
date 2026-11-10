//! Panel-by-panel construction with automatic format selection, improvement moves and a
//! deterministic multi-start search.

use super::guillotine::decompose;
use super::model::*;
use super::packer::*;
use std::time::Instant;

pub struct Problem<'a> {
    pub pieces: &'a [Piece],
    pub kinds: usize,
    pub specs: &'a [BinSpec],
}

/// Lexicographic objective: material first, then number of panels, then concentrate the waste
/// (fuller panels plus one large remnant are better than evenly half-empty panels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    pub area: f64,
    pub panels: usize,
    pub concentration: f64,
}

impl Score {
    pub fn of(bins: &[PackedBin]) -> Self {
        Self {
            area: bins.iter().map(|b| b.spec.panel_area).sum(),
            panels: bins.len(),
            concentration: bins.iter().map(|b| b.efficiency().powi(2)).sum(),
        }
    }

    pub fn better_than(&self, other: &Score) -> bool {
        if (self.area - other.area).abs() > 1.0 {
            return self.area < other.area;
        }
        if self.panels != other.panels {
            return self.panels < other.panels;
        }
        self.concentration > other.concentration + 1e-9
    }
}

pub struct SearchOptions {
    pub max_iterations: usize,
    pub deadline: Instant,
    /// Search stops as soon as a plan reaches this bound (it is optimal then).
    pub lower_bound_area: f64,
}

pub struct SearchOutcome {
    pub bins: Vec<PackedBin>,
    pub iterations: usize,
}

impl<'a> Problem<'a> {
    pub fn search(&self, options: &SearchOptions) -> SearchOutcome {
        let mut best: Option<(Score, Vec<PackedBin>)> = None;
        let mut iterations = 0;
        for iteration in 0..options.max_iterations.max(1) {
            if iteration > 0 && Instant::now() >= options.deadline {
                break;
            }
            iterations += 1;
            let mut rng = Rng::new(iteration as u64 + 1);
            // Iteration 0 mixes all formats, the next ones start from each single format (the
            // improvement moves may still mix in other formats), later ones are randomised.
            let single = self.specs.len() > 1;
            let deterministic = if single {
                iteration <= self.specs.len()
            } else {
                iteration == 0
            };
            let specs: Vec<&BinSpec> = if single && (1..=self.specs.len()).contains(&iteration) {
                vec![&self.specs[iteration - 1]]
            } else if single && !deterministic && rng.unit() < 0.3 {
                vec![&self.specs[rng.below(self.specs.len())]]
            } else {
                self.specs.iter().collect()
            };
            let params = self.params_for(deterministic, &mut rng);
            let selection_noise = if deterministic {
                0.0
            } else {
                0.03 * rng.unit()
            };
            let mut bins = self.construct(&specs, &params, selection_noise, &mut rng);
            self.improve(&mut bins, &params);
            let score = Score::of(&bins);
            if best.as_ref().is_none_or(|(s, _)| score.better_than(s)) {
                best = Some((score, bins));
            }
            if best
                .as_ref()
                .is_some_and(|(s, _)| s.area <= options.lower_bound_area + 1.0)
            {
                break;
            }
        }
        let (_, mut bins) = best.expect("at least one iteration runs");
        sort_fullest_first(&mut bins);
        SearchOutcome { bins, iterations }
    }

    pub(super) fn deterministic_params(&self) -> PackParams {
        self.params_for(true, &mut Rng::new(1))
    }

    fn params_for(&self, deterministic: bool, rng: &mut Rng) -> PackParams {
        PackParams {
            strip_candidates: if deterministic { 8 } else { 3 + rng.below(10) },
            kind_noise: (0..self.kinds)
                .map(|_| {
                    if deterministic {
                        1.0
                    } else {
                        1.0 + 0.12 * (rng.unit() - 0.5)
                    }
                })
                .collect(),
            lookahead: if deterministic { 1 } else { 1 + rng.below(2) },
        }
    }

    /// Best packing of `avail` on a single panel over all formats and methods.
    /// Returns (bin, remaining pieces).
    fn best_single_bin(
        &self,
        params: &PackParams,
        avail: &Avail,
        specs: &[&BinSpec],
        selection_noise: f64,
        rng: &mut Rng,
    ) -> Option<(PackedBin, Avail)> {
        let packer = Packer {
            pieces: self.pieces,
            params,
        };
        let mut best: Option<(f64, PackedBin, Avail)> = None;
        for spec in specs {
            for method in METHODS {
                let mut rest = avail.clone();
                let placed = packer.pack(spec, method, &mut rest);
                if placed.is_empty() {
                    continue;
                }
                let bin = PackedBin::new((*spec).clone(), placed, self.pieces);
                let mut value = bin.efficiency();
                if rest.is_empty() {
                    // Everything fits: prefer the cheapest panel.
                    value += 10.0 - bin.spec.panel_area / 1e12;
                }
                if selection_noise > 0.0 {
                    value *= 1.0 + selection_noise * rng.unit();
                }
                if best.as_ref().is_none_or(|(v, _, _)| value > *v) {
                    best = Some((value, bin, rest));
                }
            }
        }
        best.map(|(_, bin, rest)| (bin, rest))
    }

    fn construct(
        &self,
        specs: &[&BinSpec],
        params: &PackParams,
        selection_noise: f64,
        rng: &mut Rng,
    ) -> Vec<PackedBin> {
        let mut avail = Avail::new(self.pieces, self.kinds, 0..self.pieces.len());
        let all: Vec<&BinSpec> = self.specs.iter().collect();
        let mut bins = Vec::new();
        while !avail.is_empty() {
            // Pieces that do not fit the restricted formats fall back to all formats.
            let Some((bin, rest)) = self
                .best_single_bin(params, &avail, specs, selection_noise, rng)
                .or_else(|| self.best_single_bin(params, &avail, &all, selection_noise, rng))
            else {
                break; // only possible if a piece fits no panel; validated beforehand
            };
            bins.push(bin);
            avail = rest;
        }
        bins
    }

    /// Improvement moves: empty weak panels into the offcuts of the others, re-pack pairs of weak
    /// panels together and move panels to smaller formats.
    pub(super) fn improve(&self, bins: &mut Vec<PackedBin>, params: &PackParams) {
        loop {
            let mut changed = self.eliminate_weakest(bins, params);
            changed |= self.repack_weak_pair(bins, params);
            changed |= self.downsize(bins, params);
            if !changed {
                break;
            }
        }
    }

    fn weakest_order(bins: &[PackedBin]) -> Vec<usize> {
        let mut order: Vec<usize> = (0..bins.len()).collect();
        order.sort_by(|&a, &b| {
            bins[a]
                .efficiency()
                .partial_cmp(&bins[b].efficiency())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        order
    }

    /// Tries to move every piece of the weakest panel into free offcuts of the other panels.
    fn eliminate_weakest(&self, bins: &mut Vec<PackedBin>, params: &PackParams) -> bool {
        if bins.len() < 2 {
            return false;
        }
        let packer = Packer {
            pieces: self.pieces,
            params,
        };
        for victim in Self::weakest_order(bins).into_iter().take(2) {
            let mut avail = Avail::new(
                self.pieces,
                self.kinds,
                bins[victim].placed.iter().map(|p| p.piece),
            );
            let mut updated: Vec<(usize, Vec<Placed>)> = Vec::new();
            for (i, bin) in bins.iter().enumerate() {
                if i == victim || avail.is_empty() {
                    continue;
                }
                let region = Rect {
                    x: 0,
                    y: 0,
                    w: bin.spec.w,
                    h: bin.spec.h,
                };
                let Some(dec) = decompose(&bin.placed, region) else {
                    continue;
                };
                let added = packer.fill(dec.free, false, SplitRule::MaxArea, &mut avail);
                if !added.is_empty() {
                    updated.push((i, added));
                }
            }
            if avail.is_empty() {
                for (i, added) in updated {
                    let mut placed = bins[i].placed.clone();
                    placed.extend(added);
                    bins[i] = PackedBin::new(bins[i].spec.clone(), placed, self.pieces);
                }
                bins.remove(victim);
                return true;
            }
        }
        false
    }

    /// Re-packs the pieces of the two weakest panels together with all formats.
    fn repack_weak_pair(&self, bins: &mut Vec<PackedBin>, params: &PackParams) -> bool {
        if bins.len() < 2 {
            return false;
        }
        let order = Self::weakest_order(bins);
        let (a, b) = (order[0], order[1]);
        let pieces: Vec<usize> = bins[a]
            .placed
            .iter()
            .chain(bins[b].placed.iter())
            .map(|p| p.piece)
            .collect();
        let old = Score::of(&[bins[a].clone(), bins[b].clone()]);
        let specs: Vec<&BinSpec> = self.specs.iter().collect();
        let mut avail = Avail::new(self.pieces, self.kinds, pieces);
        let mut rng = Rng::new(7);
        let mut fresh = Vec::new();
        while !avail.is_empty() && fresh.len() <= 2 {
            let Some((bin, rest)) = self.best_single_bin(params, &avail, &specs, 0.0, &mut rng)
            else {
                return false;
            };
            fresh.push(bin);
            avail = rest;
        }
        if !avail.is_empty() || !Score::of(&fresh).better_than(&old) {
            return false;
        }
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        bins.remove(hi);
        bins.remove(lo);
        bins.extend(fresh);
        true
    }

    /// Moves a panel's pieces to the cheapest format that still holds all of them.
    fn downsize(&self, bins: &mut [PackedBin], params: &PackParams) -> bool {
        let mut changed = false;
        let mut rng = Rng::new(11);
        for slot in bins.iter_mut() {
            let cheaper: Vec<&BinSpec> = self
                .specs
                .iter()
                .filter(|s| s.panel_area < slot.spec.panel_area - 1.0)
                .collect();
            if cheaper.is_empty() {
                continue;
            }
            let avail = Avail::new(self.pieces, self.kinds, slot.placed.iter().map(|p| p.piece));
            if let Some((bin, rest)) = self.best_single_bin(params, &avail, &cheaper, 0.0, &mut rng)
            {
                if rest.is_empty() {
                    *slot = bin;
                    changed = true;
                }
            }
        }
        changed
    }
}

/// Fullest panels first, so the remnant ends up on the last panel.
pub fn sort_fullest_first(bins: &mut [PackedBin]) {
    bins.sort_by(|a, b| {
        b.efficiency()
            .partial_cmp(&a.efficiency())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}
