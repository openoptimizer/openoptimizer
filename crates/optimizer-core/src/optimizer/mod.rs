use crate::types::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};

mod bounds;
mod colgen;
mod guillotine;
mod knapsack;
mod lp;
mod model;
mod optional;
mod packer;
mod search;
mod summary;
#[cfg(test)]
mod tests;

use model::*;

/// Version of the packing engine reported in the result statistics.
pub const ALGORITHM: &str = "guillotine-cg-1";
const DEFAULT_TIME_LIMIT_MS: u64 = 3000;
const MAX_TIME_LIMIT_MS: u64 = 30000;
const MAX_ITERATIONS: usize = 48;
const MAX_LNS_STEPS: usize = 2000;
const LARGE_REQUEST_PIECES: usize = 150;

/// Packs rectangular items on panels with guillotine (panel saw) cuts.
///
/// Panels are filled one after another. For every new panel all panel types and several packing
/// methods (strip/knapsack packing in both directions and free-rectangle best fit) are evaluated
/// and the most efficient panel is kept, which selects the panel formats automatically.
/// Improvement moves empty weak panels into offcuts, re-pack weak pairs and move panels to smaller
/// formats. A deterministic multi-start search varies the tuning and keeps the best plan.
pub struct Optimizer {
    request: OptimizationRequest,
}

impl Optimizer {
    /// Validates requests and builds a new optimizer instance.
    pub fn new(request: OptimizationRequest) -> Result<Self> {
        if request.panel_types.is_empty() {
            return Err(OptimizerError::InvalidInput(
                "At least one panel type must be provided".to_string(),
            ));
        }
        if request.items.is_empty() {
            return Err(OptimizerError::InvalidInput(
                "At least one item must be provided".to_string(),
            ));
        }
        if !request.cut_width.is_finite() || request.cut_width < 0.0 {
            return Err(OptimizerError::InvalidInput(
                "cut_width must not be negative".to_string(),
            ));
        }
        for panel in &request.panel_types {
            if !panel.trimming.is_finite() || panel.trimming < 0.0 {
                return Err(OptimizerError::InvalidInput(format!(
                    "Panel '{}' has negative trimming",
                    panel.id
                )));
            }
            let usable_width = panel.width - (panel.trimming * 2.0);
            let usable_height = panel.height - (panel.trimming * 2.0);
            if !(usable_width > 0.0 && usable_height > 0.0) {
                return Err(OptimizerError::InvalidInput(format!(
                    "Panel '{}' becomes unusable after applying trimming",
                    panel.id
                )));
            }
        }
        for item in &request.items {
            let valid = item.width.is_finite()
                && item.height.is_finite()
                && item.width > 0.0
                && item.height > 0.0;
            if !valid {
                return Err(OptimizerError::InvalidInput(format!(
                    "Item '{}' must have a positive width and height",
                    item.id
                )));
            }
        }
        if request.items.iter().all(|i| i.quantity == 0) {
            return Err(OptimizerError::InvalidInput(
                "At least one item must have a quantity".to_string(),
            ));
        }
        Ok(Self { request })
    }

    fn kerf(&self) -> i64 {
        to_units(self.request.cut_width)
    }

    /// Expands quantities into single pieces (ids `<id>_<n>` when quantity > 1).
    fn expand_pieces(&self) -> (Vec<Piece>, usize) {
        let kerf = self.kerf();
        let mut kinds: HashMap<(i64, i64, bool), usize> = HashMap::new();
        let mut pieces = Vec::new();
        for item in &self.request.items {
            let w = to_units(item.width) + kerf;
            let h = to_units(item.height) + kerf;
            let next = kinds.len();
            let kind = *kinds.entry((w, h, item.can_rotate)).or_insert(next);
            for i in 0..item.quantity {
                pieces.push(Piece {
                    id: if item.quantity > 1 {
                        format!("{}_{}", item.id, i + 1)
                    } else {
                        item.id.clone()
                    },
                    w,
                    h,
                    can_rotate: item.can_rotate,
                    real_area: to_units(item.width) * to_units(item.height),
                    kind,
                });
            }
        }
        (pieces, kinds.len())
    }

    fn bin_specs(&self) -> Vec<BinSpec> {
        let kerf = self.kerf();
        self.request
            .panel_types
            .iter()
            .enumerate()
            .map(|(i, p)| BinSpec {
                type_index: i,
                w: to_units(p.width - 2.0 * p.trimming) + kerf,
                h: to_units(p.height - 2.0 * p.trimming) + kerf,
                panel_area: p.width * p.height,
            })
            .collect()
    }

    /// Executes the full optimization flow and returns packed layouts.
    pub fn optimize(&self) -> Result<OptimizationResult> {
        let started = Instant::now();
        let (pieces, kinds) = self.expand_pieces();
        let specs = self.bin_specs();

        for piece in &pieces {
            let fits = specs
                .iter()
                .any(|s| piece.orientations(false).any(|o| o.w <= s.w && o.h <= s.h));
            if !fits {
                return Err(OptimizerError::ItemDoesNotFit(piece.id.clone()));
            }
        }

        let time_limit = self
            .request
            .time_limit_ms
            .unwrap_or(DEFAULT_TIME_LIMIT_MS)
            .min(MAX_TIME_LIMIT_MS);
        let limit = Duration::from_millis(time_limit);
        let problem = search::Problem {
            pieces: &pieces,
            kinds,
            specs: &specs,
        };

        // 1. Multi-start construction and improvement search. Large requests keep most of the
        //    time here because a single LP dive over hundreds of pieces is slow.
        let search_share = if pieces.len() > LARGE_REQUEST_PIECES {
            0.85
        } else {
            0.4
        };
        let simple_bound =
            bounds::round_up_to_panels(bounds::lower_bound_area(&pieces, &specs), &specs);
        let outcome = problem.search(&search::SearchOptions {
            max_iterations: MAX_ITERATIONS,
            deadline: started + limit.mul_f64(search_share),
            lower_bound_area: simple_bound,
        });
        let mut bins = outcome.bins;
        let mut iterations = outcome.iterations;
        let mut strategy = "search";
        let incumbent = search::Score::of(&bins).area;

        // 2. Column generation at the root: proven lower bound and a pool of good patterns.
        //    It stops as soon as the bound proves the search result optimal.
        let mut columns = colgen::ColumnGeneration::new(&problem);
        let lower_bound_area = if incumbent <= simple_bound + 1.0 {
            simple_bound
        } else {
            let lp_bound = columns.root(started + limit.mul_f64(search_share + 0.15), |bound| {
                bounds::round_up_to_panels(bound, &specs) >= incumbent - 1.0
            });
            bounds::round_up_to_panels(simple_bound.max(lp_bound), &specs)
        };

        // 3. Large neighbourhood search through the pattern LP: a full dive first, then a few
        //    weak panels at a time are dissolved and their pieces re-packed by diving.
        if search::Score::of(&bins).area > lower_bound_area + 1.0 {
            columns.add_bins(&bins);
            let deadline = started + limit;
            let params = problem.deterministic_params();
            let mut rng = Rng::new(97);
            for step in 0..MAX_LNS_STEPS {
                if Instant::now() >= deadline {
                    break;
                }
                // Later steps may use at most a third of the remaining time.
                let now = Instant::now();
                let step_deadline = now + deadline.saturating_duration_since(now) / 3;
                let candidate = if step == 0 {
                    columns.dive_all(&mut rng, 0.0, deadline)
                } else {
                    let k = (2 + rng.below(3)).min(bins.len());
                    let mut order: Vec<(f64, usize)> = bins
                        .iter()
                        .enumerate()
                        .map(|(i, b)| (b.efficiency() * (1.0 + 0.6 * rng.unit()), i))
                        .collect();
                    order
                        .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    let chosen: Vec<usize> = order.iter().take(k).map(|o| o.1).collect();
                    let victims: Vec<PackedBin> = chosen.iter().map(|&i| bins[i].clone()).collect();
                    columns
                        .repack(&victims, step % 2 == 0, &mut rng, 0.5, step_deadline)
                        .map(|fresh| {
                            let mut plan: Vec<PackedBin> = bins
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| !chosen.contains(i))
                                .map(|(_, b)| b.clone())
                                .collect();
                            plan.extend(fresh);
                            plan
                        })
                };
                let Some(mut candidate) = candidate else {
                    continue;
                };
                iterations += 1;
                problem.improve(&mut candidate, &params);
                let score = search::Score::of(&candidate);
                if score.better_than(&search::Score::of(&bins)) {
                    search::sort_fullest_first(&mut candidate);
                    bins = candidate;
                    strategy = "column-generation";
                    columns.add_bins(&bins);
                }
                if score.area <= lower_bound_area + 1.0 {
                    break;
                }
            }
        }
        let outcome = search::SearchOutcome { bins, iterations };
        if outcome.bins.iter().map(|b| b.placed.len()).sum::<usize>() != pieces.len() {
            return Err(OptimizerError::CannotFitAll);
        }

        let mut extra: Vec<Vec<optional::OptionalPlacement>> = vec![Vec::new(); outcome.bins.len()];
        let mut built = self.to_layouts(&outcome.bins, &pieces, &extra);
        let summary = self.calculate_summary(&built.layouts);
        let effective_waste = summary
            .actual_waste_percentage
            .unwrap_or(summary.waste_percentage);
        if effective_waste > optional::MIN_WASTE_THRESHOLD_PERCENT {
            extra = self.place_optional_items(&outcome.bins);
            if extra.iter().any(|e| !e.is_empty()) {
                built = self.to_layouts(&outcome.bins, &pieces, &extra);
            }
        }
        let optional_items_used = extra.iter().flatten().map(|p| p.id.clone()).collect();

        let panels_required = self.count_panels(&built.layouts);
        let summary = self.calculate_summary(&built.layouts);

        Ok(OptimizationResult {
            panels_required,
            layouts: built.layouts,
            summary,
            optional_items_used,
            statistics: Some(Statistics {
                algorithm: ALGORITHM.to_string(),
                iterations: outcome.iterations as u32,
                elapsed_ms: started.elapsed().as_millis() as u64,
                guillotine: built.guillotine,
                max_stage: built.max_stage,
                lower_bound_area,
                strategy: strategy.to_string(),
            }),
        })
    }

    /// Converts internal bins into output layouts including cuts and offcuts.
    fn to_layouts(
        &self,
        bins: &[PackedBin],
        pieces: &[Piece],
        extra: &[Vec<optional::OptionalPlacement>],
    ) -> BuiltLayouts {
        let kerf = self.kerf();
        let mut numbers: HashMap<usize, u32> = HashMap::new();
        let mut guillotine_ok = true;
        let mut max_stage = 0;
        let mut layouts = Vec::with_capacity(bins.len());
        for (i, bin) in bins.iter().enumerate() {
            let panel = &self.request.panel_types[bin.spec.type_index];
            let number = numbers.entry(bin.spec.type_index).or_insert(0);
            *number += 1;
            let t = panel.trimming;
            let mut placed = bin.placed.clone();
            let mut ids: Vec<String> = bin
                .placed
                .iter()
                .map(|p| pieces[p.piece].id.clone())
                .collect();
            for e in extra.get(i).into_iter().flatten() {
                placed.push(e.placed);
                ids.push(e.id.clone());
            }
            let decomposition = match guillotine::decompose(&placed, full_rect(&bin.spec)) {
                Some(d) => d,
                None => {
                    guillotine_ok = false;
                    guillotine::Decomposition::default()
                }
            };
            max_stage = max_stage.max(decomposition.stages);
            let placements = placed
                .iter()
                .zip(ids)
                .map(|(p, id)| Placement {
                    item_id: id,
                    x: t + to_mm(p.x),
                    y: t + to_mm(p.y),
                    width: to_mm(p.w - kerf),
                    height: to_mm(p.h - kerf),
                    rotated: p.rotated,
                })
                .collect();
            let min_side = to_units(10.0);
            let mut unused_areas: Vec<UnusedArea> = decomposition
                .free
                .iter()
                .filter(|r| r.w - kerf >= min_side && r.h - kerf >= min_side)
                .map(|r| UnusedArea {
                    x: t + to_mm(r.x),
                    y: t + to_mm(r.y),
                    width: to_mm(r.w - kerf),
                    height: to_mm(r.h - kerf),
                })
                .collect();
            unused_areas.sort_by(|a, b| {
                (b.width * b.height)
                    .partial_cmp(&(a.width * a.height))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let cuts = decomposition
                .cuts
                .iter()
                .map(|c| {
                    let along = t + to_mm(c.pos - kerf);
                    let from = t + to_mm(c.from);
                    let length = to_mm(c.to - c.from - kerf);
                    if c.vertical {
                        Cut {
                            stage: c.stage,
                            direction: CutDirection::Vertical,
                            x: along,
                            y: from,
                            length,
                        }
                    } else {
                        Cut {
                            stage: c.stage,
                            direction: CutDirection::Horizontal,
                            x: from,
                            y: along,
                            length,
                        }
                    }
                })
                .collect();
            layouts.push(PanelLayout {
                panel_type_id: panel.id.clone(),
                panel_number: *number,
                width: panel.width,
                height: panel.height,
                trimming: t,
                placements,
                unused_areas,
                cuts,
            });
        }
        BuiltLayouts {
            layouts,
            guillotine: guillotine_ok,
            max_stage,
        }
    }
}

struct BuiltLayouts {
    layouts: Vec<PanelLayout>,
    guillotine: bool,
    max_stage: u32,
}

fn full_rect(spec: &BinSpec) -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: spec.w,
        h: spec.h,
    }
}
