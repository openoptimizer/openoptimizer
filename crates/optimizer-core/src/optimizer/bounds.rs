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
