//! Lower bounds for the total panel area. A plan whose area equals the bound is optimal.

use super::model::*;

/// Largest of two valid bounds:
/// * area cover: the cheapest integer combination of panels whose usable areas hold the
///   (kerf-inflated) piece area;
/// * large pieces: a piece longer than half the usable width and half the usable height in every
///   orientation of every format cannot share a panel with another such piece.
pub fn lower_bound_area(pieces: &[Piece], specs: &[BinSpec]) -> f64 {
    area_cover(pieces, specs).max(large_pieces(pieces, specs))
}

fn area_cover(pieces: &[Piece], specs: &[BinSpec]) -> f64 {
    let total: f64 = pieces.iter().map(|p| to_mm2(p.inflated_area())).sum();
    if total <= 0.0 || specs.is_empty() {
        return 0.0;
    }
    // Coarse steps keep the table small; rounding the need down and the capacities up keeps the
    // bound valid.
    let step = (total / 20_000.0).max(10_000.0);
    let need = (total / step).floor() as usize;
    let types: Vec<(usize, f64)> = specs
        .iter()
        .map(|s| {
            (
                (to_mm2(s.w * s.h) / step).ceil().max(1.0) as usize,
                s.panel_area,
            )
        })
        .collect();
    let mut cost = vec![f64::INFINITY; need + 1];
    cost[0] = 0.0;
    for covered in 1..=need {
        for &(capacity, area) in &types {
            let rest = covered.saturating_sub(capacity);
            cost[covered] = cost[covered].min(cost[rest] + area);
        }
    }
    cost[need]
}

fn large_pieces(pieces: &[Piece], specs: &[BinSpec]) -> f64 {
    pieces
        .iter()
        .filter_map(|piece| {
            let mut cheapest: Option<f64> = None;
            for spec in specs {
                let mut fits = false;
                for o in piece.orientations(false) {
                    if o.w <= spec.w && o.h <= spec.h {
                        fits = true;
                        if o.w * 2 <= spec.w || o.h * 2 <= spec.h {
                            // Could share this format with another large piece.
                            return None;
                        }
                    }
                }
                if fits {
                    cheapest = Some(cheapest.map_or(spec.panel_area, |c| c.min(spec.panel_area)));
                }
            }
            cheapest
        })
        .sum()
}

/// Smallest total area of whole panels that is at least `bound`: every plan consists of whole
/// panels, so a fractional bound can be rounded up to the next achievable combination.
pub fn round_up_to_panels(bound: f64, specs: &[BinSpec]) -> f64 {
    let mut areas: Vec<f64> = specs.iter().map(|s| s.panel_area).collect();
    areas.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    areas.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    // Tolerance for floating point noise in the LP bound.
    let target = bound * (1.0 - 1e-7) - 1.0;
    if target <= 0.0 || areas.is_empty() {
        return bound.max(0.0);
    }
    let mut best = f64::INFINITY;
    let mut budget: u32 = 200_000;
    cover(&areas, 0, target, 0.0, &mut best, &mut budget);
    if budget == 0 || !best.is_finite() {
        bound
    } else {
        best.max(bound)
    }
}

fn cover(areas: &[f64], i: usize, target: f64, sum: f64, best: &mut f64, budget: &mut u32) {
    if *budget == 0 {
        return;
    }
    *budget -= 1;
    if sum >= target {
        *best = best.min(sum);
        return;
    }
    let area = areas[i];
    let needed = ((target - sum) / area).ceil().max(0.0) as u64;
    if i + 1 == areas.len() {
        *best = best.min(sum + needed as f64 * area);
        return;
    }
    for count in (0..=needed).rev() {
        let next = sum + count as f64 * area;
        if next >= *best {
            continue;
        }
        cover(areas, i + 1, target, next, best, budget);
    }
}
